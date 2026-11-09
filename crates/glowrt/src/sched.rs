//! Processes inside one request: spawn, send, selective receive, links, monitors,
//! exit signals and receive timeouts.
//!
//! All processes share the request heap (messages are not copied; terms are
//! immutable, so this is only visible in memory use). Each process has its own Y
//! stack. A process that waits leaves its frames on its stack and a list of resume
//! records (function table index, arity, dispatch block, entry sp, sp), innermost
//! last. The scheduler loop in the generated `main` asks `rt_sched` what to do next
//! and re-enters the top record's function through the table.
//!
//! Time is virtual: a receive timeout fires only when no process can run, which gives
//! erl's results for programs whose outcome does not depend on wall-clock races.

use crate::atoms::*;
use crate::build;
use crate::exc;
use crate::mem;
use crate::roots;
use crate::term::*;
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::collections::VecDeque;

#[derive(Clone, Copy)]
pub struct Rec {
    pub tidx: u32,
    pub ab: u32, // arity << 16 | dispatch block
    pub entry_sp: u32,
    pub sp: u32,
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum Status {
    New,
    Runnable,
    Waiting,
    Exited,
}

pub struct P {
    pid: u32,
    status: Status,
    stack_base: u32,
    stack_top: u32,
    own_stack: bool,
    recs: Vec<Rec>,
    mailbox: Vec<Term>,
    save: usize,
    start_fun: Term,
    start_args: Term,
    links: Vec<u32>,
    /// monitors held by others on this process: (ref, watcher pid)
    watched_by: Vec<(Term, Term)>, // (ref, watcher pid term, maybe remote)
    trap_exit: bool,
    pdict: Term,
    deadline: Option<u64>,
    timed_out: bool,
    queued: bool,
    /// changed since the last checkpoint (node mode)
    dirty: bool,
    /// gen_server-shaped: {Mod, State} from the last snapshot, for restarts across deploys
    gs: Term,
    /// restored from a checkpoint of another build without a state: exits code_upgrade
    pending_kill: bool,
}

const SPAWN_STACK_WORDS: usize = 8 * 1024;

pub struct Sched {
    procs: Vec<P>,
    cur: usize,
    runq: VecDeque<usize>,
    mark: usize,
    clock: u64,
    registered: Vec<(Term, u32)>,
    /// process aliases: (ref, owner pid)
    aliases: Vec<(Term, u32)>,
    /// tier 2: messages for other nodes, (node atom, destination, message)
    outbox: Vec<(Term, Term, Term)>,
    /// monitors on processes in other nodes: (ref, node atom, target, watcher pid)
    remote_mons: Vec<(Term, Term, Term, u32)>,
    node_name: Term,
    crash: bool,
    /// erlang timers: (ref, deadline ms, destination, message as delivered)
    timers: Vec<(Term, u64, Term, Term)>,
    res: Rec,
    res_val: Term,
    main_result: Term,
    main: usize,
    /// tier 2: processes, heap and arena persist across turns
    node: bool,
    next_pid: u32,
    dead: Vec<u32>,
    ckpt: Vec<(u32, Vec<u8>)>,
}

static mut S: Sched = Sched {
    procs: Vec::new(),
    cur: 0,
    runq: VecDeque::new(),
    mark: 0,
    clock: 0,
    registered: Vec::new(),
    aliases: Vec::new(),
    outbox: Vec::new(),
    remote_mons: Vec::new(),
    node_name: NONODE,
    crash: false,
    timers: Vec::new(),
    res: Rec { tidx: 0, ab: 0, entry_sp: 0, sp: 0 },
    res_val: NIL,
    main_result: 0,
    main: 0,
    node: false,
    next_pid: 2,
    dead: Vec::new(),
    ckpt: Vec::new(),
};

fn s() -> &'static mut Sched {
    unsafe { &mut *core::ptr::addr_of_mut!(S) }
}

const MAIN_PID: u32 = 0;

fn new_proc(pid: u32, base: u32, top: u32, own: bool) -> P {
    P {
        pid,
        status: Status::Runnable,
        stack_base: base,
        stack_top: top,
        own_stack: own,
        recs: Vec::new(),
        mailbox: Vec::new(),
        save: 0,
        start_fun: NIL,
        start_args: NIL,
        links: Vec::new(),
        watched_by: Vec::new(),
        trap_exit: false,
        pdict: NIL,
        deadline: None,
        timed_out: false,
        queued: false,
        dirty: true,
        gs: UNDEFINED,
        pending_kill: false,
    }
}

/// Start of a request: only the main process, on the main stack.
pub unsafe fn reset() {
    let s = s();
    for p in s.procs.drain(..) {
        free_stack(&p);
    }
    s.procs.push(new_proc(MAIN_PID, mem::STACK_BASE, mem::STACK_TOP, false));
    s.cur = 0;
    s.runq.clear();
    s.mark = 0;
    s.clock = 0;
    s.registered.clear();
    s.aliases.clear();
    s.outbox.clear();
    s.remote_mons.clear();
    s.crash = false;
    s.timers.clear();
    s.res_val = NIL;
    s.main_result = 0;
    s.main = 0;
    s.next_pid = 2;
    s.dead.clear();
    s.ckpt.clear();
}

/// Node mode, start of a turn: drop exited processes and start a new main process on
/// the main stack. Everything else (processes, heap, arena) carries over.
pub unsafe fn new_turn() {
    let s = s();
    if s.cur < s.procs.len() && s.procs[s.cur].status != Status::Exited {
        s.procs[s.cur].pdict = roots::proc().pdict;
    }
    roots::proc().pdict = NIL;
    s.procs.retain(|p| p.status != Status::Exited);
    s.runq.clear();
    for p in s.procs.iter_mut() {
        p.queued = false;
    }
    let id = s.next_pid;
    s.next_pid += 1;
    let mut m = new_proc(id, mem::STACK_BASE, mem::STACK_TOP, false);
    m.dirty = false;
    s.procs.push(m);
    s.main = s.procs.len() - 1;
    s.cur = s.main;
    s.res_val = NIL;
    s.main_result = 0;
    // processes made runnable by restore (upgrades) or still runnable
    for i in 0..s.procs.len() {
        if i != s.main && matches!(s.procs[i].status, Status::New | Status::Runnable) {
            enqueue(i);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rt_node_mode() {
    reset();
    let s = s();
    s.procs.clear();
    s.cur = usize::MAX; // no process has run yet: new_turn must not save a dictionary
    s.node = true;
}
pub fn node_mode() -> bool {
    s().node
}

unsafe fn free_stack(p: &P) {
    if p.own_stack && p.stack_base != 0 {
        let words = (p.stack_top - p.stack_base) as usize / 8;
        core::ptr::write_bytes(p.stack_base as usize as *mut u8, 0, words * 8);
        dealloc(p.stack_base as usize as *mut u8, Layout::from_size_align(words * 8, 8).unwrap());
    }
}

fn find(pid: u32) -> Option<usize> {
    s().procs.iter().position(|p| p.pid == pid && p.status != Status::Exited)
}

fn cur() -> &'static mut P {
    let s = s();
    &mut s.procs[s.cur]
}

pub fn self_pid() -> Term {
    pid(cur().pid as u64)
}

pub fn current_stack_top() -> u32 {
    cur().stack_top
}

/// GC roots: every process's mailbox, dictionary and start closure, and the stacks
/// of the processes that are not running (the running one's is scanned from sp).
pub unsafe fn for_each_root(f: &mut dyn FnMut(&mut Term)) {
    let s = s();
    for (i, p) in s.procs.iter_mut().enumerate() {
        if p.status == Status::Exited {
            continue;
        }
        for m in p.mailbox.iter_mut() {
            f(m);
        }
        if i != s.cur {
            f(&mut p.pdict);
        }
        f(&mut p.start_fun);
        f(&mut p.start_args);
        f(&mut p.gs);
        for (r, w) in p.watched_by.iter_mut() {
            f(r);
            f(w);
        }
        if i != s.cur {
            if let Some(lo) = p.recs.iter().map(|r| r.sp).filter(|&x| x != 0).min() {
                let mut y = lo as usize as *mut u64;
                let top = p.stack_top as usize as *mut u64;
                while y < top {
                    f(&mut *y);
                    y = y.add(1);
                }
            }
        }
    }
    for (n, _) in s.registered.iter_mut() {
        f(n);
    }
    for (r, _) in s.aliases.iter_mut() {
        f(r);
    }
    for (n, d, m) in s.outbox.iter_mut() {
        f(n);
        f(d);
        f(m);
    }
    for (r, n, t, _) in s.remote_mons.iter_mut() {
        f(r);
        f(n);
        f(t);
    }
    for (r, _, d, m) in s.timers.iter_mut() {
        f(r);
        f(d);
        f(m);
    }
    f(&mut s.res_val);
    if s.main_result != 0 {
        f(&mut s.main_result);
    }
}

fn enqueue(i: usize) {
    let s = s();
    if !s.procs[i].queued {
        s.procs[i].queued = true;
        s.runq.push_back(i);
    }
}

unsafe fn deliver(i: usize, msg: Term) {
    let p = &mut s().procs[i];
    p.mailbox.push(msg);
    p.dirty = true;
    if p.status == Status::Waiting {
        p.status = Status::Runnable;
        enqueue(i);
    }
}

// --------------------------------------------------------------------------------
// Exits

unsafe fn exit_reason_from_exception() -> Term {
    let p = roots::proc();
    let class = p.exc_class;
    let reason = p.exc_reason;
    let t = p.exc_trace;
    let trace = if is_tuple(t) && tuple_arity(t) == 2 { tuple_get(t, 1) } else { NIL };
    if class == EXIT {
        reason
    } else if class == THROW {
        build::tuple(&[build::tuple(&[NOCATCH, reason]), trace])
    } else {
        build::tuple(&[reason, trace])
    }
}

/// Process `i` exits with `reason`: signals to linked processes and monitors.
unsafe fn do_exit(i: usize, reason: Term) {
    let s = s();
    let me = s.procs[i].pid;
    s.procs[i].status = Status::Exited;
    if s.node && i != s.main {
        s.dead.push(me);
    }
    s.procs[i].recs.clear();
    s.procs[i].mailbox.clear();
    free_stack(&s.procs[i]);
    s.procs[i].stack_base = 0;
    let links = core::mem::take(&mut s.procs[i].links);
    let watchers = core::mem::take(&mut s.procs[i].watched_by);
    s.registered.retain(|&(_, p)| p != me);
    s.aliases.retain(|&(_, p)| p != me);
    for (r, w) in watchers {
        let m = build::tuple(&[DOWN, r, PROCESS, pid(me as u64), reason]);
        if is_remote(w) {
            outbox_push(w, m);
        } else if let Some(j) = find(pid_val(w)) {
            deliver(j, m);
        }
    }
    for l in links {
        if let Some(j) = find(l) {
            s.procs[j].links.retain(|&x| x != me);
            exit_signal(j, me, reason);
        }
    }
    if i == s.main && s.main_result == 0 {
        // main killed by a signal: report it as an exit exception
        exc::raise(EXIT, reason);
    }
}

/// An exit signal from `from` reaches process `j`.
unsafe fn exit_signal(j: usize, from: u32, reason: Term) {
    let p = &s().procs[j];
    if reason == KILL {
        do_exit(j, KILLED);
    } else if p.trap_exit {
        let m = build::tuple(&[EXIT_UPPER, pid(from as u64), reason]);
        deliver(j, m);
    } else if reason != NORMAL {
        do_exit(j, reason);
    }
}

// --------------------------------------------------------------------------------
// The scheduler loop's side (called from the generated main)

/// Called by `main` each time a run segment ends with value `v`.
/// Returns 0: done (main finished, result via rt_main_result), 1: resume the record
/// in rt_res_*, 2: start a new process (rt_start_fun / rt_start_args).
#[no_mangle]
pub unsafe extern "C" fn rt_sched(v: Term) -> i32 {
    let r = rt_sched_(v);
    if crate::bifs::debug_enabled() {
        let s = s();
        crate::host_log(&format!("sched v={} susp -> {} cur={} res=({}, ab={:#x}, esp={:#x}, sp={:#x}) recs={:?}", crate::fmt::show(if v == 0 { NIL } else { v }), r, s.procs[s.cur].pid, s.res.tidx, s.res.ab, s.res.entry_sp, s.res.sp, s.procs[s.cur].recs.iter().map(|r| (r.tidx, r.ab)).collect::<Vec<_>>()));
    }
    r
}

unsafe fn rt_sched_(v: Term) -> i32 {
    let s = s();
    let st = &mut *core::ptr::addr_of_mut!(mem::STATE);
    if st.suspended != 0 {
        st.suspended = 0;
        let p = &mut s.procs[s.cur];
        let n = p.recs.len();
        p.recs[s.mark..n].reverse();
        if p.status == Status::Runnable {
            enqueue(s.cur);
        }
    } else if let Some(r) = s.procs[s.cur].recs.pop() {
        s.res = r;
        s.res_val = v;
        return 1;
    } else {
        let i = s.cur;
        let me = s.procs[i].pid;
        let _ = me;
        if i == s.main {
            s.main_result = v;
            // main is done: the request ends, like erl's -eval halting. In node mode
            // the other processes keep running until they all wait (end of the turn).
            if v != 0 || s.node {
                s.procs[i].status = Status::Exited;
            }
            if s.node {
                // the turn's main process is gone: its names and aliases go with it
                let me = s.procs[i].pid;
                s.registered.retain(|&(_, p)| p != me);
                s.aliases.retain(|&(_, p)| p != me);
                for p in s.procs.iter_mut() {
                    p.links.retain(|&x| x != me);
                }
            }
            if !s.node || s.runq.is_empty() {
                return 0;
            }
        } else {
            let reason = if v == 0 { exit_reason_from_exception() } else { NORMAL };
            do_exit(i, reason);
            if s.procs[s.main].status == Status::Exited && !s.node {
                s.main_result = 0;
                return 0;
            }
        }
    }
    // Save the dictionary of the process that stopped.
    let pd = roots::proc().pdict;
    s.procs[s.cur].pdict = pd;
    loop {
        if let Some(i) = s.runq.pop_front() {
            s.procs[i].queued = false;
            match s.procs[i].status {
                Status::Exited | Status::Waiting => continue,
                _ => {}
            }
            switch_to(i);
            let p = &mut s.procs[i];
            if p.status == Status::New {
                p.status = Status::Runnable;
                return 2;
            }
            let r = p.recs.pop().unwrap();
            s.res = r;
            s.res_val = NIL;
            return 1;
        }
        if s.procs[s.main].status == Status::Exited {
            // node mode: main is done and every other process waits: end of the turn
            return 0;
        }
        // Nothing can run: in tier 1 (virtual time) an erlang timer that is due before
        // every receive timeout fires first.
        if !s.node {
            let rt = s.procs.iter().filter(|p| p.status == Status::Waiting).filter_map(|p| p.deadline).min();
            if let Some(at) = s.timers.iter().map(|t| t.1).min() {
                if rt.map_or(true, |d| at <= d) {
                    s.clock = s.clock.max(at);
                    fire_due(s.clock);
                    continue;
                }
            }
        }
        // Nothing can run: fire the earliest receive timeout.
        let next = s
            .procs
            .iter()
            .enumerate()
            .filter(|(_, p)| p.status == Status::Waiting && p.deadline.is_some())
            .min_by_key(|(_, p)| p.deadline.unwrap())
            .map(|(i, _)| i);
        match next {
            Some(i) => {
                let p = &mut s.procs[i];
                s.clock = s.clock.max(p.deadline.unwrap());
                p.deadline = None;
                p.timed_out = true;
                p.status = Status::Runnable;
                enqueue(i);
            }
            None => {
                // Deadlock: main waits forever. erl would hang; report it.
                let m = s.main;
                switch_to(m);
                exc::raise(ERROR, crate::atoms::intern("glowc_deadlock"));
                s.main_result = 0;
                return 0;
            }
        }
    }
}

unsafe fn switch_to(i: usize) {
    let s = s();
    s.cur = i;
    s.procs[i].dirty = true;
    roots::proc().pdict = s.procs[i].pdict;
}

#[no_mangle]
pub unsafe extern "C" fn rt_main_result() -> Term {
    s().main_result
}
#[no_mangle]
pub unsafe extern "C" fn rt_res_tidx() -> u32 {
    s().res.tidx
}
#[no_mangle]
pub unsafe extern "C" fn rt_res_arity() -> u32 {
    s().res.ab >> 16
}
#[no_mangle]
pub unsafe extern "C" fn rt_res_block() -> u32 {
    s().res.ab & 0xffff
}
#[no_mangle]
pub unsafe extern "C" fn rt_res_sp(cur_sp: u32) -> u32 {
    // 0: an outer frame, resumed after its callee returned; the callee left sp right.
    if s().res.sp == 0 { cur_sp } else { s().res.sp }
}
#[no_mangle]
pub unsafe extern "C" fn rt_res_entry_sp() -> u32 {
    s().res.entry_sp
}
#[no_mangle]
pub unsafe extern "C" fn rt_res_val() -> Term {
    let s = s();
    let v = s.res_val;
    s.res_val = NIL;
    v
}
#[no_mangle]
pub unsafe extern "C" fn rt_cur_stack_base() -> u32 {
    cur().stack_base
}
#[no_mangle]
pub unsafe extern "C" fn rt_cur_stack_top() -> u32 {
    cur().stack_top
}
#[no_mangle]
pub unsafe extern "C" fn rt_start_fun() -> Term {
    let p = cur();
    let f = p.start_fun;
    p.start_fun = NIL;
    f
}
#[no_mangle]
pub unsafe extern "C" fn rt_start_args() -> Term {
    let p = cur();
    let a = p.start_args;
    p.start_args = NIL;
    a
}

/// A call returned 0 with `suspended` set: record this frame (innermost first).
#[no_mangle]
pub unsafe extern "C" fn rt_susp_push(tidx: u32, ab: u32, entry_sp: u32, _sp: u32) {
    // An outer frame: g_sp still includes the frames of the callees, so no sp here.
    cur().recs.push(Rec { tidx, ab, entry_sp, sp: 0 });
}

unsafe fn begin_suspend(tidx: u32, ab: u32, entry_sp: u32, sp: u32) {
    let s = s();
    let p = &mut s.procs[s.cur];
    s.mark = p.recs.len();
    p.recs.push(Rec { tidx, ab, entry_sp, sp });
    (*core::ptr::addr_of_mut!(mem::STATE)).suspended = 1;
}

// --------------------------------------------------------------------------------
// receive

/// loop_rec: the message at the save pointer, or 0 when there is none.
#[no_mangle]
pub unsafe extern "C" fn rt_loop_rec() -> Term {
    let p = cur();
    if p.save < p.mailbox.len() {
        p.mailbox[p.save]
    } else {
        0
    }
}
#[no_mangle]
pub unsafe extern "C" fn rt_loop_rec_end() {
    cur().save += 1;
}
#[no_mangle]
pub unsafe extern "C" fn rt_remove_message() {
    let p = cur();
    p.mailbox.remove(p.save);
    p.save = 0;
    p.deadline = None;
    p.timed_out = false;
}
#[no_mangle]
pub unsafe extern "C" fn rt_timeout() {
    let p = cur();
    p.save = 0;
    p.deadline = None;
    p.timed_out = false;
}

/// wait: no message matched; suspend until one arrives. Resumes at the loop_rec block.
#[no_mangle]
pub unsafe extern "C" fn rt_wait(tidx: u32, ab: u32, entry_sp: u32, sp: u32) {
    cur().status = Status::Waiting;
    begin_suspend(tidx, ab, entry_sp, sp);
}

/// wait_timeout: 1 = suspended (resumes at the block after it), 0 = the timeout is 0
/// and has expired: continue with the `timeout` instruction. 0xFFFFFFFF = badarg.
#[no_mangle]
pub unsafe extern "C" fn rt_wait_timeout(t: Term, tidx: u32, ab: u32, entry_sp: u32, sp: u32) -> i32 {
    let clock = s().clock;
    let p = cur();
    if t != INFINITY {
        if !is_small(t) || small_val(t) < 0 {
            exc::error(crate::atoms::TIMEOUT_VALUE);
            return -1;
        }
        let ms = small_val(t) as u64;
        if ms == 0 {
            p.timed_out = true;
            return 0;
        }
        if p.deadline.is_none() {
            p.deadline = Some(clock + ms);
        }
    }
    p.status = Status::Waiting;
    begin_suspend(tidx, ab, entry_sp, sp);
    1
}

/// After a wait_timeout resumes: 1 if the timeout fired (run `timeout`), 0 if a
/// message arrived (go back to loop_rec).
#[no_mangle]
pub unsafe extern "C" fn rt_take_timed_out() -> i32 {
    let p = cur();
    let t = p.timed_out;
    p.timed_out = false;
    t as i32
}

// --------------------------------------------------------------------------------
// BIFs

macro_rules! bif {
    ($name:literal, fn $f:ident($($a:ident),*) $body:block) => {
        #[export_name = $name]
        pub unsafe extern "C" fn $f($($a: Term),*) -> Term $body
    };
}

unsafe fn resolve_dest(d: Term) -> Option<Option<usize>> {
    if is_pid(d) {
        return Some(find(pid_val(d)));
    }
    if is_atom(d) {
        return s().registered.iter().find(|&&(n, _)| n == d).map(|&(_, p)| find(p));
    }
    if is_tuple(d) && tuple_arity(d) == 2 && is_atom(tuple_get(d, 0)) {
        return resolve_dest(tuple_get(d, 0));
    }
    if is_ref(d) {
        // an alias; a message to an inactive alias is dropped, like ERTS
        let owner = s().aliases.iter().find(|(r, _)| crate::cmp::exact_eq(*r, d)).map(|&(_, p)| p);
        return Some(owner.and_then(find));
    }
    None
}

fn has_opt(opts: Term, name: &str) -> bool {
    let n = crate::atoms::intern(name);
    unsafe {
        build::list_to_vec(opts).unwrap_or_default().iter().any(|&o| {
            o == n || (is_tuple(o) && tuple_arity(o) == 2 && tuple_get(o, 0) == n)
        })
    }
}

/// monitor/3 alias options: {alias, explicit_unalias | demonitor | reply_demonitor}.
unsafe fn alias_opt(opts: Term) -> bool {
    let a = crate::atoms::intern("alias");
    build::list_to_vec(opts).unwrap_or_default().iter().any(|&o| is_tuple(o) && tuple_arity(o) == 2 && tuple_get(o, 0) == a)
}

#[no_mangle]
pub unsafe extern "C" fn rt_send(d: Term, msg: Term) -> Term {
    if let Some((node, dest)) = remote_dest(d) {
        s().outbox.push((node, dest, msg));
        return msg;
    }
    match resolve_dest(d) {
        Some(Some(i)) => {
            deliver(i, msg);
            msg
        }
        Some(None) if is_pid(d) => msg, // dead pid: dropped
        _ => exc::badarg(),
    }
}
bif!("erlang:send/2", fn send2(d, m) { rt_send(d, m) });
bif!("erlang:send/3", fn send3(d, m, _o) { rt_send(d, m); OK });
bif!("erlang:self/0", fn self0() { self_pid() });

unsafe fn spawn_common(fun: Term, args: Term, link: bool, monitor: bool) -> Term {
    let s = s();
    let words = SPAWN_STACK_WORDS;
    let base = alloc_zeroed(Layout::from_size_align(words * 8, 8).unwrap()) as usize as u32;
    let top = base + (words * 8) as u32;
    let id = s.next_pid;
    s.next_pid += 1;
    let mut p = new_proc(id, base, top, true);
    p.status = Status::New;
    p.start_fun = fun;
    p.start_args = args;
    let me = s.procs[s.cur].pid;
    if link {
        p.links.push(me);
    }
    let mut r = NIL;
    if monitor {
        r = crate::bifs::make_ref();
        p.watched_by.push((r, pid(me as u64)));
    }
    s.procs.push(p);
    let i = s.procs.len() - 1;
    if link {
        s.procs[s.cur].links.push(id);
    }
    enqueue(i);
    if monitor {
        build::tuple(&[pid(id as u64), r])
    } else {
        pid(id as u64)
    }
}

unsafe fn mfa_fun(m: Term, f: Term, a: Term) -> Option<(Term, Term)> {
    // spawn(M, F, A): start through apply/3 by storing {M, F} and A; the driver uses
    // apply/2 with a fun, so build the external fun here.
    if !is_atom(m) || !is_atom(f) {
        return None;
    }
    let args = build::list_to_vec(a)?;
    let fun = crate::funs::export_fun(m, f, args.len() as u32)?;
    Some((fun, a))
}

bif!("erlang:spawn/1", fn spawn1(f) { spawn_common(f, NIL, false, false) });
bif!("erlang:spawn_link/1", fn spawn_link1(f) { spawn_common(f, NIL, true, false) });
bif!("proc_lib:spawn_link/1", fn pl_spawn_link1(f) { spawn_common(f, NIL, true, false) });
bif!("proc_lib:spawn/1", fn pl_spawn1(f) { spawn_common(f, NIL, false, false) });
bif!("erlang:spawn/3", fn spawn3(m, f, a) {
    match mfa_fun(m, f, a) { Some((fun, a)) => spawn_common(fun, a, false, false), None => exc::badarg() }
});
bif!("erlang:spawn_link/3", fn spawn_link3(m, f, a) {
    match mfa_fun(m, f, a) { Some((fun, a)) => spawn_common(fun, a, true, false), None => exc::badarg() }
});
bif!("proc_lib:spawn_link/3", fn pl_spawn_link3(m, f, a) { spawn_link3(m, f, a) });
bif!("erlang:spawn_monitor/1", fn spawn_monitor1(f) { spawn_common(f, NIL, false, true) });
bif!("erlang:spawn_opt/2", fn spawn_opt2(f, opts) {
    spawn_common(f, NIL, has_opt(opts, "link"), has_opt(opts, "monitor"))
});

bif!("erlang:link/1", fn link1(p) {
    if !is_pid(p) { return exc::badarg(); }
    let s = s();
    let me = s.procs[s.cur].pid;
    let other = pid_val(p);
    if other == me { return TRUE; }
    match find(other) {
        Some(j) => {
            if !s.procs[j].links.contains(&me) { s.procs[j].links.push(me); }
            s.procs[j].dirty = true;
            let c = s.cur;
            if !s.procs[c].links.contains(&other) { s.procs[c].links.push(other); }
            TRUE
        }
        None => {
            if s.procs[s.cur].trap_exit {
                let m = build::tuple(&[EXIT_UPPER, p, NOPROC]);
                deliver(s.cur, m);
                TRUE
            } else {
                exc::raise(EXIT, NOPROC)
            }
        }
    }
});
bif!("erlang:unlink/1", fn unlink1(p) {
    if !is_pid(p) { return exc::badarg(); }
    let s = s();
    let me = s.procs[s.cur].pid;
    let other = pid_val(p);
    let c = s.cur;
    s.procs[c].links.retain(|&x| x != other);
    if let Some(j) = find(other) { s.procs[j].links.retain(|&x| x != me); s.procs[j].dirty = true; }
    TRUE
});

unsafe fn monitor_proc(target: Term) -> Term {
    let s = s();
    let me = s.procs[s.cur].pid;
    let r = crate::bifs::make_ref();
    if let Some((node, dest)) = remote_dest(target) {
        // Registered on the target's side; noconnection fires here if delivery fails.
        s.remote_mons.push((r, node, target, me));
        let ctl = build::tuple(&[intern("$glowc_monitor"), r, pid(me as u64)]);
        s.outbox.push((node, dest, ctl));
        return r;
    }
    let dest = if is_pid(target) { Some(find(pid_val(target))) } else { resolve_dest(target) };
    match dest {
        Some(Some(j)) => {
            s.procs[j].watched_by.push((r, pid(me as u64)));
            s.procs[j].dirty = true;
        }
        _ => {
            let m = build::tuple(&[DOWN, r, PROCESS, target, NOPROC]);
            deliver(s.cur, m);
        }
    }
    r
}
bif!("erlang:monitor/2", fn monitor2(t, p) {
    if t != PROCESS { return exc::badarg(); }
    monitor_proc(p)
});
bif!("erlang:monitor/3", fn monitor3(t, p, o) {
    if t != PROCESS { return exc::badarg(); }
    let r = monitor_proc(p);
    if alias_opt(o) {
        let me = cur().pid;
        s().aliases.push((r, me));
    }
    r
});
bif!("erlang:alias/0", fn alias0() {
    let r = crate::bifs::make_ref();
    let me = cur().pid;
    s().aliases.push((r, me));
    r
});
bif!("erlang:alias/1", fn alias1(_o) { alias0() });
bif!("erlang:unalias/1", fn unalias(r) {
    let s = s();
    let n = s.aliases.len();
    s.aliases.retain(|(x, _)| !crate::cmp::exact_eq(*x, r));
    boolean(s.aliases.len() != n)
});
bif!("erlang:spawn_opt/4", fn spawn_opt4(m, f, a, opts) {
    match mfa_fun(m, f, a) {
        Some((fun, a)) => spawn_common(fun, a, has_opt(opts, "link"), has_opt(opts, "monitor")),
        None => exc::badarg(),
    }
});
unsafe fn demonitor_(r: Term, flush: bool) -> Term {
    let s = s();
    s.aliases.retain(|(x, _)| !crate::cmp::exact_eq(*x, r));
    s.remote_mons.retain(|(x, _, _, _)| !crate::cmp::exact_eq(*x, r));
    for p in s.procs.iter_mut() {
        let n = p.watched_by.len();
        p.watched_by.retain(|&(x, _)| !crate::cmp::exact_eq(x, r));
        if p.watched_by.len() != n { p.dirty = true; }
    }
    if flush {
        let c = s.cur;
        s.procs[c].mailbox.retain(|&m| {
            !(is_tuple(m) && tuple_arity(m) == 5 && tuple_get(m, 0) == DOWN && crate::cmp::exact_eq(tuple_get(m, 1), r))
        });
        s.procs[c].save = 0;
    }
    TRUE
}
bif!("erlang:demonitor/1", fn demonitor1(r) { demonitor_(r, false) });
bif!("erlang:demonitor/2", fn demonitor2(r, o) {
    let o = build::list_to_vec(o).unwrap_or_default();
    demonitor_(r, o.contains(&FLUSH))
});

bif!("erlang:exit/2", fn exit2(p, reason) {
    let s = s();
    let me = s.procs[s.cur].pid;
    let Some(dest) = resolve_dest(p) else { return exc::badarg() };
    match dest {
        Some(j) if j == s.cur => {
            // A signal to ourselves: like an exit exception unless trapped.
            if reason == KILL { return exc::raise(EXIT, KILLED); }
            if s.procs[j].trap_exit {
                let m = build::tuple(&[EXIT_UPPER, pid(me as u64), reason]);
                deliver(j, m);
                TRUE
            } else if reason == NORMAL {
                TRUE
            } else {
                exc::raise(EXIT, reason)
            }
        }
        Some(j) => { exit_signal(j, me, reason); TRUE }
        None => TRUE,
    }
});

bif!("erlang:process_flag/2", fn process_flag(f, v) {
    if f == TRAP_EXIT {
        let p = cur();
        let old = p.trap_exit;
        p.trap_exit = v == TRUE;
        return boolean(old);
    }
    // other flags: accepted, old value reported as false
    FALSE
});
bif!("erlang:is_process_alive/1", fn is_alive(p) {
    if !is_pid(p) { return exc::badarg(); }
    boolean(find(pid_val(p)).is_some())
});
bif!("erlang:register/2", fn register(n, p) {
    let s = s();
    if !is_atom(n) || n == UNDEFINED || !is_pid(p) || find(pid_val(p)).is_none() || s.registered.iter().any(|&(x, q)| x == n || q == pid_val(p)) {
        return exc::badarg();
    }
    s.registered.push((n, pid_val(p)));
    if let Some(j) = find(pid_val(p)) { s.procs[j].dirty = true; }
    TRUE
});
bif!("erlang:unregister/1", fn unregister(n) {
    let s = s();
    for &(x, q) in s.registered.iter() {
        if x == n { if let Some(j) = find(q) { s.procs[j].dirty = true; } }
    }
    let before = s.registered.len();
    s.registered.retain(|&(x, _)| x != n);
    if s.registered.len() == before { return exc::badarg(); }
    TRUE
});
bif!("erlang:whereis/1", fn whereis(n) {
    match s().registered.iter().find(|&&(x, _)| x == n) {
        Some(&(_, p)) if find(p).is_some() => pid(p as u64),
        _ => UNDEFINED,
    }
});
bif!("erlang:registered/0", fn registered() {
    let v: Vec<Term> = s().registered.iter().map(|&(n, _)| n).collect();
    build::list(&v)
});
bif!("erlang:process_info/2", fn process_info(p, item) {
    if !is_pid(p) { return exc::badarg(); }
    let Some(j) = find(pid_val(p)) else { return UNDEFINED };
    let pr = &s().procs[j];
    let v = if item == MESSAGE_QUEUE_LEN {
        small(pr.mailbox.len() as i64)
    } else if item == TRAP_EXIT {
        boolean(pr.trap_exit)
    } else if item == LINKS {
        let l: Vec<Term> = pr.links.iter().map(|&x| pid(x as u64)).collect();
        build::list(&l)
    } else if item == STATUS {
        if j == s().cur { RUNNING } else if pr.status == Status::Waiting { WAITING } else { RUNNABLE }
    } else if item == REGISTERED_NAME {
        match s().registered.iter().find(|&&(_, q)| q == pr.pid) {
            Some(&(n, _)) => n,
            None => return NIL,
        }
    } else if item == DICTIONARY {
        if j == s().cur { roots::proc().pdict } else { pr.pdict }
    } else {
        return UNDEFINED;
    };
    build::tuple(&[item, v])
});
bif!("erlang:yield/0", fn yield0() { TRUE });

// --------------------------------------------------------------------------------
// Tier 2 checkpoints (node mode)
//
// A checkpoint row per process: {glowc_proc, Pid, Status, Recs, Stack, Mailbox, Save,
// Pdict, Links, WatchedBy, TrapExit, Names, StartFun, StartArgs, Gs}, as ETF. Stack is
// the process's Y stack from its lowest frame to the top (words that are not terms,
// such as zeroed slots and catch tags, as {'$glowc_raw', Word}); record sps are
// offsets from the stack top. Resume records are only valid for the same build, so a
// restore into another build restarts gen_server-shaped processes from Gs through
// glowc_node:upgrade/3, and the rest exit with reason code_upgrade.

const META_PID: u32 = u32::MAX;

unsafe fn raw_word(w: u64) -> Term {
    if w == 0 || (w & 0x3F) == 0x1B {
        build::tuple(&[intern("$glowc_raw"), small(w as i64)])
    } else {
        w
    }
}

unsafe fn unraw(t: Term) -> u64 {
    if is_tuple(t) && tuple_arity(t) == 2 && tuple_get(t, 0) == intern("$glowc_raw") {
        small_val(tuple_get(t, 1)) as u64
    } else {
        t
    }
}

unsafe fn encode_proc(i: usize) -> Option<Vec<u8>> {
    let s = s();
    let p = &s.procs[i];
    let top = p.stack_top;
    let off = |x: u32| if x == 0 { 0 } else { (top - x) as i64 };
    let recs: Vec<Term> = p
        .recs
        .iter()
        .map(|r| build::tuple(&[small(r.tidx as i64), small(r.ab as i64), small(off(r.entry_sp)), small(off(r.sp))]))
        .collect();
    let lo = p.recs.iter().map(|r| r.sp).filter(|&x| x != 0).min().unwrap_or(top);
    let mut words = Vec::new();
    let mut y = lo as usize as *const u64;
    while (y as usize) < top as usize {
        words.push(raw_word(*y));
        y = y.add(1);
    }
    let status = match p.status {
        Status::Waiting => WAITING,
        Status::New => intern("new"),
        _ => RUNNABLE,
    };
    let names: Vec<Term> = s.registered.iter().filter(|&&(_, q)| q == p.pid).map(|&(n, _)| n).collect();
    let links: Vec<Term> = p.links.iter().map(|&x| pid(x as u64)).collect();
    let watched: Vec<Term> = p.watched_by.iter().map(|&(r, w)| build::tuple(&[r, w])).collect();
    let pdict = if i == s.cur { roots::proc().pdict } else { p.pdict };
    let t = build::tuple(&[
        intern("glowc_proc"),
        pid(p.pid as u64),
        status,
        build::list(&recs),
        build::list(&words),
        build::list(&p.mailbox),
        small(p.save as i64),
        pdict,
        build::list(&links),
        build::list(&watched),
        boolean(p.trap_exit),
        build::list(&names),
        p.start_fun,
        p.start_args,
        p.gs,
    ]);
    crate::etf::term_to_binary(t)
}

/// End of a turn: encodes every dirty process (and the node's counters). Returns the
/// number of rows; read them with rt_ckpt_pid / rt_ckpt_blob, deleted pids with
/// rt_dead_count / rt_dead_pid.
#[no_mangle]
pub unsafe extern "C" fn rt_checkpoint() -> u32 {
    let s = s();
    s.ckpt.clear();
    for i in 0..s.procs.len() {
        if i == s.main || s.procs[i].status == Status::Exited || !s.procs[i].dirty {
            continue;
        }
        match encode_proc(i) {
            Some(b) => {
                s.procs[i].dirty = false;
                let pid = s.procs[i].pid;
                s.ckpt.push((pid, b));
            }
            None => crate::host_log(&format!("checkpoint: process <0.{}.0> holds a term ETF cannot encode", s.procs[i].pid)),
        }
    }
    let timers: Vec<Term> = s.timers.iter().map(|&(r, at, d, m)| build::tuple(&[r, build::int(at as i64), d, m])).collect();
    let meta = build::tuple(&[intern("glowc_meta"), small(s.next_pid as i64), small(roots::proc().refs as i64), small(s.clock as i64), build::list(&timers)]);
    if let Some(b) = crate::etf::term_to_binary(meta) {
        s.ckpt.push((META_PID, b));
    }
    s.ckpt.len() as u32
}
#[no_mangle]
pub unsafe extern "C" fn rt_ckpt_pid(i: u32) -> u32 {
    s().ckpt[i as usize].0
}
#[no_mangle]
pub unsafe extern "C" fn rt_ckpt_blob(i: u32) -> u64 {
    let b = &s().ckpt[i as usize].1;
    let p = mem::balloc(b.len());
    core::ptr::copy_nonoverlapping(b.as_ptr(), p, b.len());
    ((p as usize as u64) << 32) | b.len() as u64
}
#[no_mangle]
pub unsafe extern "C" fn rt_dead_count() -> u32 {
    s().dead.len() as u32
}
#[no_mangle]
pub unsafe extern "C" fn rt_dead_pid(i: u32) -> u32 {
    s().dead[i as usize]
}
#[no_mangle]
pub unsafe extern "C" fn rt_checkpoint_done() {
    let s = s();
    s.ckpt.clear();
    s.dead.clear();
}

static mut RESTORE_BUF: (u32, u32) = (0, 0);

#[no_mangle]
pub unsafe extern "C" fn rt_restore_buf(len: u32) -> u32 {
    let p = mem::balloc(len as usize) as usize as u32;
    RESTORE_BUF = (p, len);
    p
}

/// Restores one row from rt_restore_buf. `upgrade` = the row comes from another build.
/// Returns 0 ok, 1 not decodable.
#[no_mangle]
pub unsafe extern "C" fn rt_restore(upgrade: i32) -> i32 {
    let (p, len) = RESTORE_BUF;
    let bytes = core::slice::from_raw_parts(p as usize as *const u8, len as usize);
    let Some(t) = crate::etf::binary_to_term(bytes, false) else { return 1 };
    let s = s();
    if is_tuple(t) && tuple_get(t, 0) == intern("glowc_meta") {
        s.next_pid = s.next_pid.max(small_val(tuple_get(t, 1)) as u32);
        let r = small_val(tuple_get(t, 2)) as u64;
        if roots::proc().refs < r {
            roots::proc().refs = r;
        }
        s.clock = s.clock.max(small_val(tuple_get(t, 3)) as u64);
        if tuple_arity(t) >= 5 {
            s.timers.clear();
            for x in build::list_to_vec(tuple_get(t, 4)).unwrap_or_default() {
                let at = crate::num::to_i64(tuple_get(x, 1)).unwrap_or(0) as u64;
                s.timers.push((tuple_get(x, 0), at, tuple_get(x, 2), tuple_get(x, 3)));
            }
        }
        return 0;
    }
    if !is_tuple(t) || tuple_arity(t) != 15 || tuple_get(t, 0) != intern("glowc_proc") {
        return 1;
    }
    let f = |k: usize| tuple_get(t, k);
    let id = pid_val(f(1));
    let words = SPAWN_STACK_WORDS;
    let base = alloc_zeroed(Layout::from_size_align(words * 8, 8).unwrap()) as usize as u32;
    let top = base + (words * 8) as u32;
    let mut np = new_proc(id, base, top, true);
    np.dirty = false;
    let at = |o: i64| if o == 0 { 0 } else { top - o as u32 };
    let names = build::list_to_vec(f(11)).unwrap_or_default();
    np.mailbox = build::list_to_vec(f(5)).unwrap_or_default();
    np.save = small_val(f(6)) as usize;
    np.pdict = f(7);
    np.links = build::list_to_vec(f(8)).unwrap_or_default().iter().map(|&x| pid_val(x)).collect();
    np.watched_by = build::list_to_vec(f(9))
        .unwrap_or_default()
        .iter()
        .map(|&x| (tuple_get(x, 0), tuple_get(x, 1)))
        .collect();
    np.trap_exit = f(10) == TRUE;
    np.gs = f(14);
    if upgrade != 0 {
        if is_tuple(np.gs) && tuple_arity(np.gs) == 2 {
            let name = names.first().copied().unwrap_or(UNDEFINED);
            let up = crate::funs::export_fun(intern("glowc_node"), intern("upgrade"), 3).unwrap();
            np.status = Status::New;
            np.start_fun = up;
            np.start_args = build::list(&[tuple_get(np.gs, 0), name, tuple_get(np.gs, 1)]);
            np.pdict = NIL;
            np.save = 0;
            np.dirty = true;
            // keep the name taken, so the turn's own code finds the server before it runs
            for n in names {
                s.registered.push((n, id));
            }
        } else {
            np.status = Status::Waiting;
            np.pending_kill = true;
            for n in names {
                s.registered.push((n, id));
            }
        }
    } else {
        let recs = build::list_to_vec(f(3)).unwrap_or_default();
        np.recs = recs
            .iter()
            .map(|&r| Rec {
                tidx: small_val(tuple_get(r, 0)) as u32,
                ab: small_val(tuple_get(r, 1)) as u32,
                entry_sp: at(small_val(tuple_get(r, 2))),
                sp: at(small_val(tuple_get(r, 3))),
            })
            .collect();
        let stack = build::list_to_vec(f(4)).unwrap_or_default();
        let lo = top as usize - stack.len() * 8;
        for (k, &w) in stack.iter().enumerate() {
            *((lo + 8 * k) as *mut u64) = unraw(w);
        }
        let st = f(2);
        np.status = if st == WAITING { Status::Waiting } else if st == intern("new") { Status::New } else { Status::Runnable };
        np.start_fun = f(12);
        np.start_args = f(13);
        for n in names {
            s.registered.push((n, id));
        }
    }
    s.next_pid = s.next_pid.max(id + 1);
    s.procs.push(np);
    0
}

/// After every row is restored: processes that could not be carried into this build
/// exit with code_upgrade (their links and monitors see it).
#[no_mangle]
pub unsafe extern "C" fn rt_restore_done() {
    let s = s();
    for i in 0..s.procs.len() {
        if s.procs[i].pending_kill {
            s.procs[i].pending_kill = false;
            do_exit(i, intern("code_upgrade"));
        }
    }
}

fn intern(n: &str) -> Term {
    crate::atoms::intern(n)
}

/// glowc:node_gen_servers/0: [{Pid, Mod}] for dirty processes started by proc_lib for a
/// gen-style callback module ('$initial_call' = {Mod, init, 1}).
bif!("glowc:node_gen_servers/0", fn node_gen_servers() {
    let s = s();
    let ic = intern("$initial_call");
    let init = intern("init");
    let mut out = Vec::new();
    for (i, p) in s.procs.iter().enumerate() {
        if i == s.main || p.status == Status::Exited || !p.dirty {
            continue;
        }
        let mut d = p.pdict;
        while is_list(d) {
            let kv = head(d);
            if tuple_get(kv, 0) == ic {
                let v = tuple_get(kv, 1);
                if is_tuple(v) && tuple_arity(v) == 3 && tuple_get(v, 1) == init {
                    out.push(build::tuple(&[pid(p.pid as u64), tuple_get(v, 0)]));
                }
            }
            d = tail(d);
        }
    }
    build::list(&out)
});
bif!("glowc:node_set_state/3", fn node_set_state(p, m, st) {
    if let Some(j) = find(pid_val(p)) {
        let gs = build::tuple(&[m, st]);
        s().procs[j].gs = gs;
    }
    OK
});

// --------------------------------------------------------------------------------
// Tier 2 messaging between nodes (Durable Objects)
//
// A remote pid carries its node's atom index in its high bits; ETF carries the node
// name. Sends to a remote pid or to {Name, Node} go to the outbox, which the host
// delivers after the turn as one batch per node ({deliver, [{Dest, Msg}]}). Monitors
// on remote processes are registered on the target's side with a control message; if
// the host cannot deliver to a node, it runs {noconnection, Node}, and every monitor
// on that node fires 'DOWN' with reason noconnection. Links across nodes are not
// supported.

pub fn node_name() -> Term {
    s().node_name
}

fn is_remote(p: Term) -> bool {
    let ni = pid_node_idx(p);
    ni != 0 && atom(ni) != s().node_name
}

/// (node atom, destination on that node) when `d` names a process in another node.
fn remote_dest(d: Term) -> Option<(Term, Term)> {
    unsafe {
        if is_pid(d) && is_remote(d) {
            return Some((atom(pid_node_idx(d)), pid(pid_val(d) as u64)));
        }
        if is_tuple(d) && tuple_arity(d) == 2 && is_atom(tuple_get(d, 0)) && is_atom(tuple_get(d, 1)) {
            let n = tuple_get(d, 1);
            if n != s().node_name && n != NONODE {
                return Some((n, tuple_get(d, 0)));
            }
        }
    }
    None
}

unsafe fn outbox_push(to: Term, msg: Term) {
    if let Some((node, dest)) = remote_dest(to) {
        s().outbox.push((node, dest, msg));
    }
}

#[no_mangle]
pub unsafe extern "C" fn rt_set_node(p: u32, len: u32) {
    let name = core::str::from_utf8(core::slice::from_raw_parts(p as usize as *const u8, len as usize)).unwrap_or("nonode");
    s().node_name = intern(name);
}

/// Outbox after a turn: the number of entries; then per entry rt_outbox_node (node name
/// bytes) and rt_outbox_item (ETF of {Dest, Msg}). rt_outbox_clear when sent.
#[no_mangle]
pub unsafe extern "C" fn rt_outbox_count() -> u32 {
    s().outbox.len() as u32
}
unsafe fn pack(b: &[u8]) -> u64 {
    let p = mem::balloc(b.len());
    core::ptr::copy_nonoverlapping(b.as_ptr(), p, b.len());
    ((p as usize as u64) << 32) | b.len() as u64
}
#[no_mangle]
pub unsafe extern "C" fn rt_outbox_node(i: u32) -> u64 {
    pack(crate::atoms::name(s().outbox[i as usize].0).as_bytes())
}
#[no_mangle]
pub unsafe extern "C" fn rt_outbox_item(i: u32) -> u64 {
    let (_, d, m) = s().outbox[i as usize];
    let t = build::tuple(&[d, m]);
    pack(&crate::etf::term_to_binary(t).unwrap_or_default())
}
#[no_mangle]
pub unsafe extern "C" fn rt_outbox_clear() {
    s().outbox.clear();
}
#[no_mangle]
pub unsafe extern "C" fn rt_take_crash() -> i32 {
    let s = s();
    let c = s.crash;
    s.crash = false;
    c as i32
}

/// glowc:node_deliver(Dest, Msg): a message from another node.
bif!("glowc:node_deliver/2", fn node_deliver(d, m) {
    let s = s();
    if is_tuple(m) && tuple_arity(m) == 3 && tuple_get(m, 0) == intern("$glowc_monitor") {
        let (r, w) = (tuple_get(m, 1), tuple_get(m, 2));
        match resolve_dest(d) {
            Some(Some(j)) => { s.procs[j].watched_by.push((r, w)); s.procs[j].dirty = true; }
            _ => {
                let target = if is_pid(d) { d } else { build::tuple(&[d, s.node_name]) };
                outbox_push(w, build::tuple(&[DOWN, r, PROCESS, target, NOPROC]));
            }
        }
        return OK;
    }
    if is_tuple(m) && tuple_arity(m) == 5 && tuple_get(m, 0) == DOWN {
        let r = tuple_get(m, 1);
        s.remote_mons.retain(|(x, _, _, _)| !crate::cmp::exact_eq(*x, r));
    }
    if let Some(Some(j)) = resolve_dest(d) {
        deliver(j, m);
    }
    OK
});

/// glowc:node_noconnection(Node): delivery to Node failed.
bif!("glowc:node_noconnection/1", fn node_noconnection(n) {
    let s = s();
    let mons = core::mem::take(&mut s.remote_mons);
    for (r, node, target, w) in mons {
        if node == n {
            if let Some(j) = find(w) {
                deliver(j, build::tuple(&[DOWN, r, PROCESS, target, NOCONNECTION]));
            }
        } else {
            s.remote_mons.push((r, node, target, w));
        }
    }
    OK
});

/// glowc:node_crash(): the host throws after this turn (tests: a failing node).
bif!("glowc:node_crash/0", fn node_crash() { s().crash = true; OK });

bif!("erlang:node/0", fn node0() { s().node_name });
bif!("erlang:node/1", fn node1(p) {
    if is_pid(p) && pid_node_idx(p) != 0 { atom(pid_node_idx(p)) } else { s().node_name }
});

// --------------------------------------------------------------------------------
// Timers: erlang:send_after/start_timer/cancel_timer/read_timer.
//
// Tier 1 uses the virtual clock (a timer fires when nothing can run, like a receive
// timeout). In node mode deadlines are wall-clock ms; the host arms one DO alarm for
// the earliest (rt_next_timer) and the alarm's turn fires the due ones. The table is
// part of the meta checkpoint row, so timers survive eviction.

fn now_ms() -> u64 {
    if s().node { (crate::host_now_us() / 1000.0) as u64 } else { s().clock }
}

unsafe fn fire_due(now: u64) -> u32 {
    let s = s();
    let mut due: Vec<(Term, u64, Term, Term)> = Vec::new();
    s.timers.retain(|&t| if t.1 <= now { due.push(t); false } else { true });
    due.sort_by_key(|t| t.1);
    for &(_, _, d, m) in &due {
        if let Some((node, dest)) = remote_dest(d) {
            s.outbox.push((node, dest, m));
        } else if let Some(Some(j)) = resolve_dest(d) {
            deliver(j, m);
        }
    }
    due.len() as u32
}

unsafe fn timer(t: Term, d: Term, m: Term, wrap: bool) -> Term {
    let Some(ms) = crate::num::to_i64(t) else { return exc::badarg() };
    if ms < 0 || !(is_pid(d) || is_atom(d)) { return exc::badarg(); }
    let r = crate::bifs::make_ref();
    let msg = if wrap { build::tuple(&[intern("timeout"), r, m]) } else { m };
    let at = now_ms() + ms as u64;
    s().timers.push((r, at, d, msg));
    r
}
bif!("erlang:send_after/3", fn send_after3(t, d, m) { timer(t, d, m, false) });
bif!("erlang:send_after/4", fn send_after4(t, d, m, _o) { timer(t, d, m, false) });
bif!("erlang:start_timer/3", fn start_timer3(t, d, m) { timer(t, d, m, true) });
bif!("erlang:start_timer/4", fn start_timer4(t, d, m, _o) { timer(t, d, m, true) });

unsafe fn cancel(r: Term, remove: bool) -> Term {
    if !is_ref(r) { return exc::badarg(); }
    let s = s();
    let now = now_ms();
    match s.timers.iter().position(|t| crate::cmp::exact_eq(t.0, r)) {
        Some(i) => {
            let left = s.timers[i].1.saturating_sub(now);
            if remove { s.timers.remove(i); }
            build::int(left as i64)
        }
        None => FALSE,
    }
}
bif!("erlang:cancel_timer/1", fn cancel_timer1(r) { cancel(r, true) });
bif!("erlang:cancel_timer/2", fn cancel_timer2(r, o) {
    let v = cancel(r, true);
    let info_false = build::list_to_vec(o).unwrap_or_default().iter().any(|&x| is_tuple(x) && tuple_arity(x) == 2 && tuple_get(x, 0) == intern("info") && tuple_get(x, 1) == FALSE);
    if info_false { OK } else { v }
});
bif!("erlang:read_timer/1", fn read_timer1(r) { cancel(r, false) });
bif!("erlang:read_timer/2", fn read_timer2(r, _o) { cancel(r, false) });

/// Earliest timer deadline (ms since the epoch in node mode), or -1.
#[no_mangle]
pub unsafe extern "C" fn rt_next_timer() -> f64 {
    s().timers.iter().map(|t| t.1).min().map_or(-1.0, |x| x as f64)
}

/// glowc:node_fire_timers(): fire every due timer (the alarm's turn). Returns the count.
bif!("glowc:node_fire_timers/0", fn node_fire_timers() { small(fire_due(now_ms()) as i64) });
