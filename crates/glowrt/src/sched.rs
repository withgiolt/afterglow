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
    watched_by: Vec<(Term, u32)>,
    trap_exit: bool,
    pdict: Term,
    deadline: Option<u64>,
    timed_out: bool,
    queued: bool,
}

const SPAWN_STACK_WORDS: usize = 8 * 1024;

pub struct Sched {
    procs: Vec<P>,
    cur: usize,
    runq: VecDeque<usize>,
    mark: usize,
    clock: u64,
    registered: Vec<(Term, u32)>,
    res: Rec,
    res_val: Term,
    main_result: Term,
}

static mut S: Sched = Sched {
    procs: Vec::new(),
    cur: 0,
    runq: VecDeque::new(),
    mark: 0,
    clock: 0,
    registered: Vec::new(),
    res: Rec { tidx: 0, ab: 0, entry_sp: 0, sp: 0 },
    res_val: NIL,
    main_result: 0,
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
    s.res_val = NIL;
    s.main_result = 0;
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
        for (r, _) in p.watched_by.iter_mut() {
            f(r);
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
    s.procs[i].recs.clear();
    s.procs[i].mailbox.clear();
    free_stack(&s.procs[i]);
    s.procs[i].stack_base = 0;
    let links = core::mem::take(&mut s.procs[i].links);
    let watchers = core::mem::take(&mut s.procs[i].watched_by);
    s.registered.retain(|&(_, p)| p != me);
    for (r, w) in watchers {
        if let Some(j) = find(w) {
            let m = build::tuple(&[DOWN, r, PROCESS, pid(me as u64), reason]);
            deliver(j, m);
        }
    }
    for l in links {
        if let Some(j) = find(l) {
            s.procs[j].links.retain(|&x| x != me);
            exit_signal(j, me, reason);
        }
    }
    if me == MAIN_PID && s.main_result == 0 {
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
        if me == MAIN_PID {
            s.main_result = v;
            // main is done: the request ends, like erl's -eval halting.
            if v != 0 {
                s.procs[i].status = Status::Exited;
            }
            return 0;
        }
        let reason = if v == 0 { exit_reason_from_exception() } else { NORMAL };
        do_exit(i, reason);
        if s.procs[0].status == Status::Exited {
            s.main_result = 0;
            return 0;
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
                switch_to(0);
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
    None
}

#[no_mangle]
pub unsafe extern "C" fn rt_send(d: Term, msg: Term) -> Term {
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
    let id = s.procs.iter().map(|p| p.pid).max().unwrap_or(0).max(1) + 1;
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
        p.watched_by.push((r, me));
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
    let o = build::list_to_vec(opts).unwrap_or_default();
    let link = o.contains(&crate::atoms::intern("link"));
    let mon = o.contains(&crate::atoms::intern("monitor"));
    spawn_common(f, NIL, link, mon)
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
    if let Some(j) = find(other) { s.procs[j].links.retain(|&x| x != me); }
    TRUE
});

unsafe fn monitor_proc(target: Term) -> Term {
    let s = s();
    let me = s.procs[s.cur].pid;
    let r = crate::bifs::make_ref();
    let dest = if is_pid(target) { Some(find(pid_val(target))) } else { resolve_dest(target) };
    match dest {
        Some(Some(j)) => s.procs[j].watched_by.push((r, me)),
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
bif!("erlang:monitor/3", fn monitor3(t, p, _o) {
    if t != PROCESS { return exc::badarg(); }
    monitor_proc(p)
});
unsafe fn demonitor_(r: Term, flush: bool) -> Term {
    let s = s();
    for p in s.procs.iter_mut() {
        p.watched_by.retain(|&(x, _)| !crate::cmp::exact_eq(x, r));
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
    TRUE
});
bif!("erlang:unregister/1", fn unregister(n) {
    let s = s();
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
