//! Wasm code generation for BEAM functions.
//!
//! Each BEAM function becomes `(x0..x(n-1): i64) -> i64`. X registers are locals, Y
//! registers live on a linear-memory stack addressed by the global `$sp`, float
//! registers are f64 locals. A call returns a term or 0 (an exception); the caller
//! branches to the innermost active try/catch handler (known statically) or returns 0.
//!
//! Labels become nested blocks in code order, so a forward jump is a `br`. Functions
//! with backward jumps get an outer loop and a `br_table` on a label-index local.

use crate::program::{Program, Target};
use beam_reader::{Instr, Operand};
use std::collections::{HashMap, HashSet};
use wasm_encoder::{BlockType, Function, Instruction as I, MemArg, ValType};

pub const NIL: i64 = 0x3B;

pub fn small(v: i64) -> i64 {
    (v << 4) | 0xF
}
pub fn atom(i: u32) -> i64 {
    ((i as i64) << 6) | 0x0B
}
pub fn fits_small(v: i64) -> bool {
    v >= -(1 << 59) && v < (1 << 59)
}

fn mem64(offset: u64) -> MemArg {
    MemArg { offset, align: 3, memory_index: 0 }
}
fn mem32(offset: u64) -> MemArg {
    MemArg { offset, align: 2, memory_index: 0 }
}

/// Indices the function bodies need: globals, runtime imports, types.
pub struct Env<'a> {
    pub prog: &'a Program,
    pub rt: &'a HashMap<String, u32>, // runtime import name -> function index
    pub func_base: u32,               // function index of the first compiled function
    pub type_of_arity: &'a dyn Fn(u32) -> u32,
    pub g_sp: u32,
    pub g_st: u32,
    pub g_xr: u32,
    pub g_lit: u32,
    pub g_sb: u32,
    pub lambda_table: &'a HashMap<(usize, u32), u32>,
    pub extra_literals: std::cell::RefCell<Vec<Vec<u8>>>,
    pub literal_count: u32,
    /// helper functions defined in the generated module
    pub f_alloc: u32,
    pub susp: &'a Suspend,
    /// table index of each resumable function (re-entered by the scheduler)
    pub resume_tidx: &'a HashMap<(usize, usize), u32>,
    pub g_res: u32,
}

/// Which functions may suspend: they contain `wait`/`wait_timeout`, or call one that
/// may. `ind`: a call through a fun or apply may suspend (some lambda or exported
/// function may).
pub struct Suspend {
    pub may: HashSet<(usize, usize)>,
    pub ind: bool,
}

fn has_wait(code: &[Instr]) -> bool {
    code.iter().any(|i| matches!(i.op, "wait" | "wait_timeout"))
}

fn call_may_suspend(prog: &Program, m: usize, ins: &Instr, may: &HashSet<(usize, usize)>, ind: bool) -> bool {
    match ins.op {
        "call" | "call_last" | "call_only" => match ins.args.get(1) {
            Some(Operand::Label(l)) => prog.function_by_label(m, *l).map_or(false, |fi| may.contains(&(m, fi))),
            _ => false,
        },
        "call_ext" | "call_ext_last" | "call_ext_only" => match ins.args.get(1) {
            Some(Operand::U(imp)) => {
                let (mfa, t) = prog.import_target(m, *imp as u32);
                if mfa.0 == "erlang" && mfa.1 == "apply" && (mfa.2 == 2 || mfa.2 == 3) {
                    return ind;
                }
                matches!(t, Target::Compiled(m2, f2) if may.contains(&(m2, f2)))
            }
            _ => false,
        },
        "call_fun" | "call_fun2" | "apply" | "apply_last" => ind,
        _ => false,
    }
}

pub fn suspend_analysis(prog: &Program) -> Suspend {
    let mut may: HashSet<(usize, usize)> =
        prog.functions.iter().copied().filter(|&(m, fi)| has_wait(&prog.modules[m].functions[fi].code)).collect();
    let mut indirect: HashSet<(usize, usize)> = HashSet::new();
    for &(m, li) in prog.lambdas.keys() {
        let lam = &prog.modules[m].lambdas[li as usize];
        if let Some(fi) = prog.function_by_label(m, lam.label) {
            indirect.insert((m, fi));
        }
    }
    for &(m, fi) in &prog.functions {
        let module = &prog.modules[m];
        if module.exports.iter().any(|e| e.label == module.functions[fi].entry) {
            indirect.insert((m, fi));
        }
    }
    loop {
        let ind = may.iter().any(|f| indirect.contains(f));
        let mut changed = false;
        for &(m, fi) in &prog.functions {
            if may.contains(&(m, fi)) {
                continue;
            }
            if prog.modules[m].functions[fi].code.iter().any(|ins| call_may_suspend(prog, m, ins, &may, ind)) {
                may.insert((m, fi));
                changed = true;
            }
        }
        if !changed {
            return Suspend { may, ind };
        }
    }
}

/// A non-tail call that may suspend: the caller must be able to resume after it.
fn is_susp_site(prog: &Program, m: usize, ins: &Instr, sus: &Suspend) -> bool {
    matches!(ins.op, "call" | "call_ext" | "call_fun" | "call_fun2" | "apply") && call_may_suspend(prog, m, ins, &sus.may, sus.ind)
}

/// Functions that need resume entry points.
pub fn is_resumable(prog: &Program, sus: &Suspend, m: usize, fi: usize) -> bool {
    let code = &prog.modules[m].functions[fi].code;
    has_wait(code) || code.iter().any(|ins| is_susp_site(prog, m, ins, sus))
}

impl<'a> Env<'a> {
    fn rt(&self, name: &str) -> u32 {
        *self.rt.get(name).unwrap_or_else(|| panic!("glowrt does not export {}", name))
    }
    /// A literal created by the compiler (e.g. a big integer operand).
    fn add_literal(&self, etf: Vec<u8>) -> u32 {
        let mut v = self.extra_literals.borrow_mut();
        let i = self.literal_count + v.len() as u32;
        v.push(etf);
        i
    }
}

#[derive(Debug)]
pub struct CgError(pub String);

/// Per-function state.
struct Fx<'e, 'a> {
    env: &'e Env<'a>,
    m: usize,
    f: Function,
    nparams: u32,
    nx: u32,
    // local indices
    l_tmp: u32,  // 4 i64 temps
    l_tmpi: u32, // 4 i32 temps
    l_entry_sp: u32,
    l_pc: u32,
    l_caught: u32,
    l_bs: u32, // 3 i64 temps for inline binary matching
    l_fr: u32, // float regs start
    // control
    labels: HashMap<u32, usize>, // label -> block index
    nblocks: usize,
    cur_block: usize,
    extra_depth: u32,
    dispatch: bool,
    fi_label: u32,
    handlers: Vec<u32>, // static try/catch handler labels (innermost last)
    mod_atom: i64,
    fun_atom: i64,
    arity: u32,
    /// resume table index when the function is resumable
    tidx: Option<u32>,
    /// the call being compiled may suspend: its result check comes after the resume label
    defer_check: bool,
}

fn x_index(o: &Operand) -> Option<u32> {
    match o.plain() {
        Operand::X(i) => Some(*i),
        _ => None,
    }
}

/// Highest X register / float register mentioned.
fn scan_regs(code: &[Instr]) -> (u32, u32) {
    fn walk(o: &Operand, mx: &mut u32, mf: &mut u32) {
        match o {
            Operand::X(i) => *mx = (*mx).max(*i + 1),
            Operand::FloatReg(i) => *mf = (*mf).max(*i + 1),
            Operand::TypedReg(r, _) => walk(r, mx, mf),
            Operand::List(v) => v.iter().for_each(|x| walk(x, mx, mf)),
            _ => {}
        }
    }
    let (mut mx, mut mf) = (0, 0);
    for i in code {
        for a in &i.args {
            walk(a, &mut mx, &mut mf);
        }
        // calls use x0..x(arity-1) implicitly
        if matches!(i.op, "call" | "call_last" | "call_only" | "call_ext" | "call_ext_last" | "call_ext_only" | "call_fun" | "call_fun2") {
            if let Some(Operand::U(a)) = i.args.get(if i.op == "call_fun2" { 1 } else { 0 }) {
                mx = mx.max(*a as u32 + 1);
            }
        }
        if let ("apply" | "apply_last", Some(Operand::U(a))) = (i.op, i.args.first()) {
            mx = mx.max(*a as u32 + 2).max(3);
        }
        if matches!(i.op, "try_case") {
            mx = mx.max(3);
        }
        if let ("test_heap" | "allocate_heap" | "allocate", Some(Operand::U(l))) = (i.op, i.args.last()) {
            mx = mx.max(*l as u32);
        }
    }
    (mx, mf)
}

pub fn compile_function(env: &Env, m: usize, fi: usize) -> Result<Function, CgError> {
    let prog = env.prog;
    let module = &prog.modules[m];
    let func = &module.functions[fi];
    let code = &func.code;

    // Find the func_info label (clause failure) and the body start (entry label).
    let fi_pos = code.iter().position(|i| i.op == "func_info").unwrap();
    let fi_label = code[..fi_pos]
        .iter()
        .rev()
        .find_map(|i| if i.op == "label" { if let Some(Operand::U(l)) = i.args.first() { Some(*l as u32) } else { None } } else { None })
        .unwrap_or(0);
    let mut body: Vec<Instr> = code[fi_pos + 1..].to_vec();
    let tidx = env.resume_tidx.get(&(m, fi)).copied();
    // Resumable: a label (new dispatch block) after every call that may suspend, where
    // the result check happens, and after wait_timeout, where the timeout resumes.
    let mut susp_sites: HashSet<usize> = HashSet::new();
    if tidx.is_some() {
        let mut nb = Vec::with_capacity(body.len() + 16);
        let mut syn = 0x4000_0000u64;
        for ins in body {
            let site = is_susp_site(prog, m, &ins, env.susp);
            let wt = if ins.op == "wait_timeout" { ins.args.first().cloned() } else { None };
            nb.push(ins);
            if site {
                susp_sites.insert(nb.len() - 1);
                nb.push(Instr { op: "label", args: vec![Operand::U(syn)] });
                nb.push(Instr { op: "glowc_resume_check", args: vec![] });
                syn += 1;
            }
            if let Some(l) = wt {
                nb.push(Instr { op: "label", args: vec![Operand::U(syn)] });
                nb.push(Instr { op: "glowc_timeout_resume", args: vec![l] });
                syn += 1;
            }
        }
        body = nb;
    }

    // Blocks: each label starts one. Block 0 starts at the first instruction.
    let mut labels = HashMap::new();
    let mut block_of_instr = Vec::with_capacity(body.len());
    let mut nblocks = 0usize;
    for (k, ins) in body.iter().enumerate() {
        if ins.op == "label" {
            if let Some(Operand::U(l)) = ins.args.first() {
                // consecutive labels share a block only if no instruction between; keep simple: new block
                if k == 0 {
                    nblocks = 1;
                } else {
                    nblocks += 1;
                }
                labels.insert(*l as u32, nblocks - 1);
            }
        } else if nblocks == 0 {
            nblocks = 1;
        }
        block_of_instr.push(nblocks.saturating_sub(1));
    }

    // Backward jumps? (fail label of func_info is handled inline, not a jump)
    let mut dispatch = tidx.is_some();
    for (k, ins) in body.iter().enumerate() {
        for l in jump_targets(ins) {
            if l == 0 || l == fi_label {
                continue;
            }
            match labels.get(&l) {
                Some(&b) if b <= block_of_instr[k] && !(ins.op == "label") => dispatch = true,
                None => return Err(CgError(format!("jump to unknown label {} in {}", l, crate::program::mfa_str(&prog.mfa_of(m, fi))))),
                _ => {}
            }
        }
    }

    // Static handler stack per block entry. A fault in a block after its handler's
    // label (code laid out after the try_case) jumps backward: needs dispatch.
    let handler_state = handler_analysis(&body, &labels, nblocks, &block_of_instr);
    for (b, st) in handler_state.iter().enumerate() {
        if let Some(hs) = st {
            if hs.iter().any(|h| labels.get(h).map_or(false, |&hb| hb <= b)) {
                dispatch = true;
            }
        }
    }

    let (mut nx, nf) = scan_regs(&body);
    nx = nx.max(func.arity).max(1);
    let nparams = func.arity;
    let mut locals: Vec<(u32, ValType)> = Vec::new();
    let extra_x = nx - nparams;
    locals.push((extra_x, ValType::I64));
    let l_tmp = nx;
    locals.push((4, ValType::I64));
    let l_tmpi = l_tmp + 4;
    locals.push((4, ValType::I32));
    let l_entry_sp = l_tmpi + 4;
    let l_pc = l_entry_sp + 1;
    let l_caught = l_pc + 1;
    locals.push((3, ValType::I32));
    let l_bs = l_caught + 1;
    locals.push((3, ValType::I64));
    let l_fr = l_bs + 3;
    if nf > 0 {
        locals.push((nf, ValType::F64));
    }
    let f = Function::new(locals);

    let mut fx = Fx {
        env,
        m,
        f,
        nparams,
        nx,
        l_tmp,
        l_tmpi,
        l_entry_sp,
        l_pc,
        l_caught,
        l_bs,
        l_fr,
        labels,
        nblocks,
        cur_block: 0,
        extra_depth: 0,
        dispatch,
        fi_label,
        handlers: Vec::new(),
        mod_atom: atom(prog.atom_index[module.name()]),
        fun_atom: atom(prog.atom_index[module.atom(func.name)]),
        arity: func.arity,
        tidx,
        defer_check: false,
    };


    // Prologue. A resumable function entered by the scheduler (g_res = block + 1)
    // restores its entry sp and x0 and jumps to the resume block.
    if tidx.is_some() {
        fx.ins(I::GlobalGet(env.g_res));
        fx.ins(I::If(BlockType::Empty));
        fx.ins(I::GlobalGet(env.g_res));
        fx.ins(I::I32Const(1));
        fx.ins(I::I32Sub);
        fx.ins(I::LocalSet(l_pc));
        fx.ins(I::I32Const(0));
        fx.ins(I::GlobalSet(env.g_res));
        fx.ins(I::Call(env.rt("rt_res_entry_sp")));
        fx.ins(I::LocalSet(l_entry_sp));
        fx.ins(I::Call(env.rt("rt_res_val")));
        fx.ins(I::LocalSet(0));
        fx.ins(I::Else);
        fx.ins(I::GlobalGet(env.g_sp));
        fx.ins(I::LocalSet(l_entry_sp));
        fx.gc_check(func.arity);
        fx.ins(I::I32Const(0));
        fx.ins(I::LocalSet(l_pc));
        fx.ins(I::End);
    } else {
        fx.ins(I::GlobalGet(env.g_sp));
        fx.ins(I::LocalSet(l_entry_sp));
        fx.gc_check(func.arity);
        if dispatch {
            fx.ins(I::I32Const(0));
            fx.ins(I::LocalSet(l_pc));
        }
    }
    if dispatch {
        fx.ins(I::Loop(BlockType::Empty));
    }
    for _ in 1..fx.nblocks {
        fx.ins(I::Block(BlockType::Empty));
    }
    if dispatch {
        fx.ins(I::Block(BlockType::Empty));
        fx.ins(I::LocalGet(l_pc));
        let targets: Vec<u32> = (0..fx.nblocks as u32).collect();
        fx.ins(I::BrTable(targets.into(), 0));
        fx.ins(I::End);
    }

    let mut k = 0;
    while k < body.len() {
        let ins = &body[k];
        if ins.op == "label" {
            let b = block_of_instr[k];
            if b != fx.cur_block {
                // close the block that ends right before block b
                fx.ins(I::End);
                fx.cur_block = b;
                fx.handlers = handler_state[b].clone().unwrap_or_default();
            }
            k += 1;
            continue;
        }
        fx.defer_check = susp_sites.contains(&k);
        fx.instr(ins).map_err(|e| CgError(format!("{} in {}: {:?}", e.0, crate::program::mfa_str(&prog.mfa_of(m, fi)), ins)))?;
        k += 1;
    }
    // Close the remaining blocks
    while fx.cur_block + 1 < fx.nblocks {
        fx.ins(I::End);
        fx.cur_block += 1;
        fx.ins(I::Unreachable);
    }
    if dispatch {
        fx.ins(I::End); // loop
    }
    fx.ins(I::Unreachable);
    fx.ins(I::End);
    Ok(fx.f)
}

fn jump_targets(ins: &Instr) -> Vec<u32> {
    let mut v = Vec::new();
    fn walk(o: &Operand, v: &mut Vec<u32>) {
        match o {
            Operand::Label(l) => v.push(*l),
            Operand::List(xs) => xs.iter().for_each(|x| walk(x, v)),
            _ => {}
        }
    }
    if matches!(ins.op, "call" | "call_last" | "call_only") {
        return v; // labels there are functions, not jumps
    }
    for a in &ins.args {
        walk(a, &mut v);
    }
    v
}

/// Computes the static try/catch handler stack at the start of each block.
fn handler_analysis(body: &[Instr], labels: &HashMap<u32, usize>, nblocks: usize, block_of: &[usize]) -> Vec<Option<Vec<u32>>> {
    let mut state: Vec<Option<Vec<u32>>> = vec![None; nblocks];
    state[0] = Some(Vec::new());
    // Process in code order repeatedly until stable (structure is nearly lexical).
    for _ in 0..3 {
        let mut cur: Option<Vec<u32>> = state[0].clone();
        let mut cur_block = 0;
        for (k, ins) in body.iter().enumerate() {
            let b = block_of[k];
            if b != cur_block {
                // falling into a new block
                if state[b].is_none() {
                    state[b] = cur.clone();
                }
                cur = state[b].clone();
                cur_block = b;
            }
            let Some(st) = cur.as_mut() else { continue };
            match ins.op {
                "try" | "catch" => {
                    if let Some(Operand::Label(l)) = ins.args.get(1) {
                        st.push(*l);
                        if let Some(&hb) = labels.get(l) {
                            if state[hb].is_none() {
                                state[hb] = Some(st.clone());
                            }
                        }
                    }
                }
                "try_end" | "try_case" | "catch_end" => {
                    st.pop();
                }
                _ => {}
            }
            let st = st.clone();
            for l in jump_targets(ins) {
                if let Some(&tb) = labels.get(&l) {
                    if state[tb].is_none() {
                        state[tb] = Some(st.clone());
                    }
                }
            }
            if matches!(ins.op, "return" | "jump" | "call_last" | "call_only" | "call_ext_last" | "call_ext_only" | "badmatch" | "case_end" | "if_end" | "try_case_end" | "badrecord") {
                cur = None;
            }
        }
    }
    state
}

impl<'e, 'a> Fx<'e, 'a> {
    fn ins(&mut self, i: I) {
        self.f.instruction(&i);
    }

    fn x_local(&self, i: u32) -> u32 {
        i
    }

    // --- operands --------------------------------------------------------

    fn src(&mut self, o: &Operand) -> Result<(), CgError> {
        match o.plain() {
            Operand::X(i) => self.ins(I::LocalGet(self.x_local(*i))),
            Operand::Y(i) => {
                self.ins(I::GlobalGet(self.env.g_sp));
                self.ins(I::I64Load(mem64(8 * *i as u64)));
            }
            Operand::I(v) => {
                if fits_small(*v) {
                    self.ins(I::I64Const(small(*v)));
                } else {
                    let etf = big_etf_i64(*v);
                    let li = self.env.add_literal(etf);
                    self.literal(li);
                }
            }
            Operand::BigI(bytes) => {
                let li = self.env.add_literal(big_etf_bytes(bytes));
                self.literal(li);
            }
            Operand::Atom(a) => {
                let g = self.env.prog.global_atom(self.m, *a);
                self.ins(I::I64Const(atom(g)));
            }
            Operand::Nil => self.ins(I::I64Const(NIL)),
            Operand::Char(c) => self.ins(I::I64Const(small(*c as i64))),
            Operand::Literal(i) => {
                let gi = self.env.prog.lit_index[&(self.m, *i)];
                self.literal(gi);
            }
            Operand::U(v) => self.ins(I::I64Const(small(*v as i64))),
            Operand::FloatReg(r) => {
                // boxing a float register: allocate
                self.ins(I::LocalGet(self.l_fr + r));
                self.ins(I::Call(self.env.rt("rt_box_float")));
            }
            other => return Err(CgError(format!("unsupported source operand {:?}", other))),
        }
        Ok(())
    }

    fn literal(&mut self, gi: u32) {
        self.ins(I::GlobalGet(self.env.g_lit));
        self.ins(I::I64Load(mem64(8 * gi as u64)));
    }

    /// Stores the value produced by `val` into a destination register.
    fn dst(&mut self, d: &Operand, val: impl FnOnce(&mut Self) -> Result<(), CgError>) -> Result<(), CgError> {
        match d.plain() {
            Operand::X(i) => {
                val(self)?;
                self.ins(I::LocalSet(*i));
            }
            Operand::Y(i) => {
                self.ins(I::GlobalGet(self.env.g_sp));
                val(self)?;
                self.ins(I::I64Store(mem64(8 * *i as u64)));
            }
            Operand::FloatReg(r) => {
                val(self)?;
                let fr = self.l_fr + r;
                self.ins(I::LocalSet(fr));
            }
            other => return Err(CgError(format!("unsupported destination {:?}", other))),
        }
        Ok(())
    }

    fn mov(&mut self, s: &Operand, d: &Operand) -> Result<(), CgError> {
        let s = s.clone();
        self.dst(d, |fx| fx.src(&s))
    }

    // --- control ---------------------------------------------------------

    /// br depth to reach the start of the block holding `label`.
    fn br_label(&mut self, label: u32) -> Result<(), CgError> {
        if label == self.fi_label && label != 0 {
            return self.function_clause();
        }
        let b = *self.labels.get(&label).ok_or_else(|| CgError(format!("unknown label {}", label)))?;
        if b > self.cur_block {
            let depth = (b - self.cur_block - 1) as u32 + self.extra_depth;
            self.ins(I::Br(depth));
        } else {
            // backward: through the dispatch loop
            if !self.dispatch {
                return Err(CgError("backward jump without dispatch".into()));
            }
            self.ins(I::I32Const(b as i32));
            self.ins(I::LocalSet(self.l_pc));
            let depth = (self.nblocks - 1 - self.cur_block) as u32 + self.extra_depth;
            self.ins(I::Br(depth));
        }
        Ok(())
    }

    /// `if (cond on stack) { br label }`
    fn br_if_label(&mut self, label: u32) -> Result<(), CgError> {
        self.ins(I::If(BlockType::Empty));
        self.extra_depth += 1;
        self.br_label(label)?;
        self.extra_depth -= 1;
        self.ins(I::End);
        Ok(())
    }

    /// Exception raised (0 already produced by a runtime call, dropped): go to the
    /// innermost handler or return 0.
    fn fault(&mut self) -> Result<(), CgError> {
        if let Some(&h) = self.handlers.last() {
            self.ins(I::I32Const(1));
            self.ins(I::LocalSet(self.l_caught));
            self.br_label(h)
        } else {
            self.ins(I::LocalGet(self.l_entry_sp));
            self.ins(I::GlobalSet(self.env.g_sp));
            self.ins(I::I64Const(self.mod_atom));
            self.ins(I::I64Const(self.fun_atom));
            self.ins(I::I32Const(self.arity as i32));
            self.ins(I::Call(self.env.rt("rt_frame")));
            self.ins(I::I64Const(0));
            self.ins(I::Return);
            Ok(())
        }
    }

    /// Value on the stack: if it is 0, fault; else leave it on the stack.
    fn check(&mut self) -> Result<(), CgError> {
        let t = self.l_tmp + 3;
        self.ins(I::LocalTee(t));
        self.ins(I::I64Eqz);
        self.ins(I::If(BlockType::Empty));
        self.extra_depth += 1;
        self.fault()?;
        self.extra_depth -= 1;
        self.ins(I::End);
        self.ins(I::LocalGet(t));
        Ok(())
    }

    /// Value on the stack: if 0 jump to `fail` (or fault when fail is 0).
    fn check_or_fail(&mut self, fail: u32) -> Result<(), CgError> {
        if fail == 0 {
            return self.check();
        }
        let t = self.l_tmp + 3;
        self.ins(I::LocalTee(t));
        self.ins(I::I64Eqz);
        self.br_if_label(fail)?;
        self.ins(I::LocalGet(t));
        Ok(())
    }

    fn raise_call(&mut self, name: &str, args: &[&Operand]) -> Result<(), CgError> {
        for a in args {
            self.src(a)?;
        }
        self.ins(I::Call(self.env.rt(name)));
        self.ins(I::Drop);
        self.fault()
    }

    fn raise_error2(&mut self, tag: &str, v: &Operand) -> Result<(), CgError> {
        let a = self.env.prog.atom_index[tag];
        self.ins(I::I64Const(atom(a)));
        self.src(v)?;
        self.ins(I::Call(self.env.rt("rt_error2")));
        self.ins(I::Drop);
        self.fault()
    }

    fn function_clause(&mut self) -> Result<(), CgError> {
        self.ins(I::I64Const(self.mod_atom));
        self.ins(I::I64Const(self.fun_atom));
        self.ins(I::I32Const(self.arity as i32));
        self.ins(I::Call(self.env.rt("rt_function_clause")));
        self.ins(I::Drop);
        self.fault()
    }

    // --- GC --------------------------------------------------------------

    fn gc_check(&mut self, live: u32) {
        let env = self.env;
        self.ins(I::GlobalGet(env.g_st));
        self.ins(I::I32Load(mem32(0)));
        self.ins(I::If(BlockType::Empty));
        self.extra_depth += 1;
        self.gc_now(live);
        self.extra_depth -= 1;
        self.ins(I::End);
    }

    fn gc_now(&mut self, live: u32) {
        let env = self.env;
        for i in 0..live {
            self.ins(I::GlobalGet(env.g_xr));
            self.ins(I::LocalGet(i));
            self.ins(I::I64Store(mem64(8 * i as u64)));
        }
        self.ins(I::I32Const(live as i32));
        self.ins(I::GlobalGet(env.g_sp));
        self.ins(I::Call(env.rt("rt_gc")));
        for i in 0..live {
            self.ins(I::GlobalGet(env.g_xr));
            self.ins(I::I64Load(mem64(8 * i as u64)));
            self.ins(I::LocalSet(i));
        }
    }

    /// Allocates `words` words; leaves the i32 address in tmpi[slot].
    fn alloc(&mut self, words: u32, slot: u32) {
        self.ins(I::I32Const(words as i32));
        self.ins(I::Call(self.env.f_alloc));
        self.ins(I::LocalSet(self.l_tmpi + slot));
    }

    fn store_word(&mut self, slot: u32, word: u32, val: impl FnOnce(&mut Self) -> Result<(), CgError>) -> Result<(), CgError> {
        self.ins(I::LocalGet(self.l_tmpi + slot));
        val(self)?;
        self.ins(I::I64Store(mem64(8 * word as u64)));
        Ok(())
    }

    // --- calls -----------------------------------------------------------

    fn push_args(&mut self, n: u32) {
        for i in 0..n {
            self.ins(I::LocalGet(i));
        }
    }

    fn local_fn(&self, label: u32) -> Result<u32, CgError> {
        let prog = self.env.prog;
        let fi = prog.function_by_label(self.m, label).ok_or_else(|| CgError(format!("no function at label {}", label)))?;
        let id = prog.function_id.get(&(self.m, fi)).ok_or_else(|| CgError("callee not compiled".into()))?;
        Ok(self.env.func_base + id)
    }

    /// Function index (or runtime import) for an external call; None = undef.
    fn ext_fn(&self, imp: u32) -> (crate::program::Mfa, Option<u32>) {
        let (mfa, t) = self.env.prog.import_target(self.m, imp);
        let idx = match t {
            Target::Compiled(m, fi) => Some(self.env.func_base + self.env.prog.function_id[&(m, fi)]),
            Target::Runtime(name) => Some(self.env.rt(&name)),
            Target::Undef => None,
        };
        (mfa, idx)
    }

    fn call_result(&mut self) -> Result<(), CgError> {
        if self.defer_check {
            // checked by glowc_resume_check, after the resume label
            self.defer_check = false;
        } else {
            self.check()?;
        }
        self.ins(I::LocalSet(0));
        Ok(())
    }

    /// arity << 16 | block, for a resume record
    fn resume_ab(&self, block: usize) -> i32 {
        ((self.arity << 16) | block as u32) as i32
    }

    /// Pushes the resume record arguments (table index, arity/block, entry sp, sp).
    fn resume_args(&mut self, block: usize) -> Result<(), CgError> {
        let t = self.tidx.ok_or_else(|| CgError("receive in a function that is not resumable".into()))?;
        self.ins(I::I32Const(t as i32));
        self.ins(I::I32Const(self.resume_ab(block)));
        self.ins(I::LocalGet(self.l_entry_sp));
        self.ins(I::GlobalGet(self.env.g_sp));
        Ok(())
    }

    fn undef_call(&mut self, mfa: &crate::program::Mfa) -> Result<(), CgError> {
        let prog = self.env.prog;
        self.ins(I::I64Const(atom(prog.atom_index[&mfa.0])));
        self.ins(I::I64Const(atom(prog.atom_index[&mfa.1])));
        self.ins(I::I32Const(mfa.2 as i32));
        self.ins(I::Call(self.env.rt("rt_undef")));
        self.ins(I::Drop);
        self.fault()
    }

    fn dealloc(&mut self, n: u64) {
        if n > 0 {
            self.ins(I::GlobalGet(self.env.g_sp));
            self.ins(I::I32Const(8 * n as i32));
            self.ins(I::I32Add);
            self.ins(I::GlobalSet(self.env.g_sp));
        }
    }

    fn ext_call(&mut self, arity: u32, imp: u32, tail: Option<u64>) -> Result<(), CgError> {
        let (mfa, idx) = self.ext_fn(imp);
        if mfa.0 == "erlang" && mfa.1 == "apply" && (mfa.2 == 3 || mfa.2 == 2) {
            return self.apply_call(mfa.2, tail);
        }
        match idx {
            None => {
                if let Some(d) = tail {
                    self.dealloc(d);
                }
                self.undef_call(&mfa)
            }
            Some(fidx) => {
                let is_rt = matches!(self.env.prog.targets.get(&mfa), Some(Target::Runtime(_)));
                match tail {
                    Some(d) => {
                        self.dealloc(d);
                        self.push_args(arity);
                        if is_rt {
                            // runtime BIFs don't restore sp on fault; ours is already popped
                            self.ins(I::Call(fidx));
                            self.ins(I::Return);
                        } else {
                            self.ins(I::ReturnCall(fidx));
                        }
                        Ok(())
                    }
                    None => {
                        self.push_args(arity);
                        self.ins(I::Call(fidx));
                        self.call_result()
                    }
                }
            }
        }
    }

    /// erlang:apply/2,3 through the runtime's export table.
    fn apply_call(&mut self, arity: u32, tail: Option<u64>) -> Result<(), CgError> {
        // apply/3: x0 = M, x1 = F, x2 = Args; apply/2: x0 = Fun, x1 = Args
        let name = if arity == 3 { "glowc_apply3" } else { "glowc_apply2" };
        self.push_args(arity);
        let f = self.env.rt(name);
        match tail {
            Some(d) => {
                self.dealloc(d);
                self.ins(I::ReturnCall(f));
                Ok(())
            }
            None => {
                self.ins(I::Call(f));
                self.call_result()
            }
        }
    }

    fn call_fun(&mut self, arity: u32, fun: &Operand, tail: bool) -> Result<(), CgError> {
        // tmpi0 = table index (or -1 with an exception set)
        let fun = fun.clone();
        self.src(&fun)?;
        self.ins(I::LocalSet(self.l_tmp));
        self.ins(I::LocalGet(self.l_tmp));
        self.ins(I::I32Const(arity as i32));
        self.ins(I::Call(self.env.rt("rt_fun_check")));
        self.ins(I::LocalTee(self.l_tmpi));
        self.ins(I::I32Const(0));
        self.ins(I::I32LtS);
        self.ins(I::If(BlockType::Empty));
        self.extra_depth += 1;
        self.fault()?;
        self.extra_depth -= 1;
        self.ins(I::End);
        self.push_args(arity);
        self.ins(I::LocalGet(self.l_tmp));
        self.ins(I::LocalGet(self.l_tmpi));
        let ty = (self.env.type_of_arity)(arity + 1);
        if tail {
            self.ins(I::ReturnCallIndirect { type_index: ty, table_index: 0 });
            Ok(())
        } else {
            self.ins(I::CallIndirect { type_index: ty, table_index: 0 });
            self.call_result()
        }
    }

    fn bif(&mut self, fail: u32, imp: u32, args: &[Operand], dst: &Operand) -> Result<(), CgError> {
        let (mfa, idx) = self.ext_fn(imp);
        let Some(fidx) = idx else {
            return Err(CgError(format!("BIF {} is not implemented by glowrt", crate::program::mfa_str(&mfa))));
        };
        // Inline fast paths
        let name = format!("{}:{}", mfa.0, mfa.1);
        if args.len() == 2 && (name == "erlang:+" || name == "erlang:-") {
            return self.arith_fast(&name, fidx, fail, &args[0], &args[1], dst);
        }
        let args = args.to_vec();
        self.dst(dst, |fx| {
            for a in &args {
                fx.src(a)?;
            }
            fx.ins(I::Call(fidx));
            fx.check_or_fail(fail)
        })
    }

    /// Small-integer fast path for + and -; falls back to the runtime BIF.
    fn arith_fast(&mut self, name: &str, fidx: u32, fail: u32, a: &Operand, b: &Operand, dst: &Operand) -> Result<(), CgError> {
        let (ta, tb, tr) = (self.l_tmp, self.l_tmp + 1, self.l_tmp + 2);
        self.src(a)?;
        self.ins(I::LocalSet(ta));
        self.src(b)?;
        self.ins(I::LocalSet(tb));
        // both small?
        self.ins(I::LocalGet(ta));
        self.ins(I::LocalGet(tb));
        self.ins(I::I64And);
        self.ins(I::I64Const(0xF));
        self.ins(I::I64And);
        self.ins(I::I64Const(0xF));
        self.ins(I::I64Eq);
        self.ins(I::If(BlockType::Result(ValType::I64)));
        // r = (a>>4) op (b>>4); fits in 60 bits?
        self.ins(I::LocalGet(ta));
        self.ins(I::I64Const(4));
        self.ins(I::I64ShrS);
        self.ins(I::LocalGet(tb));
        self.ins(I::I64Const(4));
        self.ins(I::I64ShrS);
        self.ins(if name == "erlang:+" { I::I64Add } else { I::I64Sub });
        self.ins(I::LocalTee(tr));
        self.ins(I::I64Const(4));
        self.ins(I::I64Shl);
        self.ins(I::I64Const(4));
        self.ins(I::I64ShrS);
        self.ins(I::LocalGet(tr));
        self.ins(I::I64Eq);
        self.ins(I::If(BlockType::Result(ValType::I64)));
        self.ins(I::LocalGet(tr));
        self.ins(I::I64Const(4));
        self.ins(I::I64Shl);
        self.ins(I::I64Const(0xF));
        self.ins(I::I64Or);
        self.ins(I::Else);
        self.ins(I::LocalGet(ta));
        self.ins(I::LocalGet(tb));
        self.ins(I::Call(fidx));
        self.ins(I::End);
        self.ins(I::Else);
        self.ins(I::LocalGet(ta));
        self.ins(I::LocalGet(tb));
        self.ins(I::Call(fidx));
        self.ins(I::End);
        self.check_or_fail(fail)?;
        self.ins(I::LocalSet(tr));
        let d = dst.clone();
        self.dst(&d, |fx| {
            fx.ins(I::LocalGet(tr));
            Ok(())
        })
    }

    // --- tests -----------------------------------------------------------

    /// Pushes an i32 condition that is true when the test PASSES, then jumps to
    /// `fail` when it is false.
    fn test_jump(&mut self, fail: u32) -> Result<(), CgError> {
        self.ins(I::I32Eqz);
        self.br_if_label(fail)
    }

    fn rt_test(&mut self, name: &str, args: &[&Operand], fail: u32) -> Result<(), CgError> {
        for a in args {
            self.src(a)?;
        }
        self.ins(I::Call(self.env.rt(name)));
        self.test_jump(fail)
    }

    fn label_of(o: &Operand) -> u32 {
        match o {
            Operand::Label(l) => *l,
            _ => 0,
        }
    }

    fn instr(&mut self, ins: &Instr) -> Result<(), CgError> {
        let a = &ins.args;
        let u = |i: usize| -> u64 {
            match a.get(i) {
                Some(Operand::U(v)) => *v,
                _ => 0,
            }
        };
        let lab = |i: usize| -> u32 { a.get(i).map(Self::label_of).unwrap_or(0) };
        match ins.op {
            "line" | "executable_line" | "debug_line" | "nif_start" | "fclearerror" | "fcheckerror"
            | "recv_marker_clear" | "recv_marker_reserve" | "recv_marker_bind" | "recv_marker_use" => {
                if ins.op == "recv_marker_reserve" {
                    // returns a marker in Dst
                    self.dst(&a[0].clone(), |fx| {
                        fx.ins(I::I64Const(NIL));
                        Ok(())
                    })?;
                }
            }
            "move" => self.mov(&a[0], &a[1])?,
            "swap" => {
                let (x, y) = (a[0].clone(), a[1].clone());
                self.src(&x)?;
                self.ins(I::LocalSet(self.l_tmp));
                self.mov(&y, &x)?;
                let t = self.l_tmp;
                self.dst(&y, |fx| {
                    fx.ins(I::LocalGet(t));
                    Ok(())
                })?;
            }
            "init_yregs" => {
                if let Operand::List(ys) = &a[0] {
                    for y in ys.clone() {
                        self.dst(&y, |fx| {
                            fx.ins(I::I64Const(NIL));
                            Ok(())
                        })?;
                    }
                }
            }
            "allocate" | "allocate_zero" | "allocate_heap" | "allocate_heap_zero" => {
                let n = u(0);
                let live = if ins.op.starts_with("allocate_heap") { u(2) } else { u(1) } as u32;
                if n > 0 {
                    let sp = self.env.g_sp;
                    // Y stack overflow: system_limit, like running out of process memory
                    self.ins(I::GlobalGet(sp));
                    self.ins(I::I32Const(8 * n as i32));
                    self.ins(I::I32Sub);
                    self.ins(I::GlobalGet(self.env.g_sb));
                    self.ins(I::I32LtU);
                    self.ins(I::If(BlockType::Empty));
                    self.extra_depth += 1;
                    let sl = self.env.prog.atom_index["system_limit"];
                    self.ins(I::I64Const(atom(sl)));
                    self.ins(I::Call(self.env.rt("rt_error")));
                    self.ins(I::Drop);
                    self.fault()?;
                    self.extra_depth -= 1;
                    self.ins(I::End);
                    self.ins(I::GlobalGet(sp));
                    self.ins(I::I32Const(8 * n as i32));
                    self.ins(I::I32Sub);
                    self.ins(I::GlobalSet(sp));
                    self.ins(I::GlobalGet(sp));
                    self.ins(I::I32Const(0));
                    self.ins(I::I32Const(8 * n as i32));
                    self.ins(I::MemoryFill(0));
                }
                self.gc_check(live);
            }
            "test_heap" => {
                let live = u(1) as u32;
                self.gc_check(live);
            }
            "deallocate" => self.dealloc(u(0)),
            "trim" => self.dealloc(u(0)),
            "return" => {
                self.ins(I::LocalGet(0));
                self.ins(I::Return);
            }
            "call" => {
                let f = self.local_fn(lab(1))?;
                self.push_args(u(0) as u32);
                self.ins(I::Call(f));
                self.call_result()?;
            }
            "call_last" => {
                let f = self.local_fn(lab(1))?;
                self.dealloc(u(2));
                self.push_args(u(0) as u32);
                self.ins(I::ReturnCall(f));
            }
            "call_only" => {
                let f = self.local_fn(lab(1))?;
                self.push_args(u(0) as u32);
                self.ins(I::ReturnCall(f));
            }
            "call_ext" => self.ext_call(u(0) as u32, u(1) as u32, None)?,
            "call_ext_last" => self.ext_call(u(0) as u32, u(1) as u32, Some(u(2)))?,
            "call_ext_only" => self.ext_call(u(0) as u32, u(1) as u32, Some(0))?,
            "bif0" => self.bif(0, u(0) as u32, &[], &a[1].clone())?,
            "bif1" => self.bif(lab(0), u(1) as u32, &[a[2].clone()], &a[3].clone())?,
            "bif2" => self.bif(lab(0), u(1) as u32, &[a[2].clone(), a[3].clone()], &a[4].clone())?,
            "bif3" => self.bif(lab(0), u(1) as u32, &[a[2].clone(), a[3].clone(), a[4].clone()], &a[5].clone())?,
            "gc_bif1" => self.bif(lab(0), u(2) as u32, &[a[3].clone()], &a[4].clone())?,
            "gc_bif2" => self.bif(lab(0), u(2) as u32, &[a[3].clone(), a[4].clone()], &a[5].clone())?,
            "gc_bif3" => self.bif(lab(0), u(2) as u32, &[a[3].clone(), a[4].clone(), a[5].clone()], &a[6].clone())?,
            "jump" => self.br_label(lab(0))?,

            // type tests
            "is_integer" => self.rt_test("rt_is_integer", &[&a[1]], lab(0))?,
            "is_float" => self.rt_test("rt_is_float", &[&a[1]], lab(0))?,
            "is_number" => self.rt_test("rt_is_number", &[&a[1]], lab(0))?,
            "is_atom" => {
                self.src(&a[1])?;
                self.ins(I::I64Const(0x3F));
                self.ins(I::I64And);
                self.ins(I::I64Const(0x0B));
                self.ins(I::I64Eq);
                self.test_jump(lab(0))?;
            }
            "is_pid" => self.rt_test("rt_is_pid", &[&a[1]], lab(0))?,
            "is_reference" => self.rt_test("rt_is_reference", &[&a[1]], lab(0))?,
            "is_port" => self.rt_test("rt_is_port", &[&a[1]], lab(0))?,
            "is_nil" => {
                self.src(&a[1])?;
                self.ins(I::I64Const(NIL));
                self.ins(I::I64Eq);
                self.test_jump(lab(0))?;
            }
            "is_nonempty_list" => {
                self.src(&a[1])?;
                self.ins(I::I64Const(3));
                self.ins(I::I64And);
                self.ins(I::I64Const(1));
                self.ins(I::I64Eq);
                self.test_jump(lab(0))?;
            }
            "is_list" => self.rt_test("rt_is_list", &[&a[1]], lab(0))?,
            "is_tuple" => self.rt_test("rt_is_tuple", &[&a[1]], lab(0))?,
            "is_binary" => self.rt_test("rt_is_binary", &[&a[1]], lab(0))?,
            "is_bitstr" => self.rt_test("rt_is_bitstring", &[&a[1]], lab(0))?,
            "is_map" => self.rt_test("rt_is_map", &[&a[1]], lab(0))?,
            "is_boolean" => self.rt_test("rt_is_boolean", &[&a[1]], lab(0))?,
            "is_function" => self.rt_test("rt_is_function", &[&a[1]], lab(0))?,
            "is_function2" => {
                let n = a[2].clone();
                self.src(&a[1])?;
                self.src(&n)?;
                self.ins(I::Call(self.env.rt("rt_is_function2")));
                self.test_jump(lab(0))?;
            }
            "test_arity" => {
                self.src(&a[1])?;
                self.ins(I::I32Const(u(2) as i32));
                self.ins(I::Call(self.env.rt("rt_test_arity")));
                self.test_jump(lab(0))?;
            }
            "is_tagged_tuple" => {
                self.src(&a[1])?;
                self.ins(I::I32Const(u(2) as i32));
                self.src(&a[3])?;
                self.ins(I::Call(self.env.rt("rt_is_tagged_tuple")));
                self.test_jump(lab(0))?;
            }
            "is_eq_exact" | "is_ne_exact" => {
                let (x, y) = (a[1].clone(), a[2].clone());
                // fast: identical words are equal; immediates differ otherwise
                self.src(&x)?;
                self.ins(I::LocalSet(self.l_tmp));
                self.src(&y)?;
                self.ins(I::LocalSet(self.l_tmp + 1));
                self.ins(I::LocalGet(self.l_tmp));
                self.ins(I::LocalGet(self.l_tmp + 1));
                self.ins(I::I64Eq);
                self.ins(I::If(BlockType::Result(ValType::I32)));
                self.ins(I::I32Const(1));
                self.ins(I::Else);
                self.ins(I::LocalGet(self.l_tmp));
                self.ins(I::LocalGet(self.l_tmp + 1));
                self.ins(I::Call(self.env.rt("rt_exact_eq")));
                self.ins(I::End);
                if ins.op == "is_ne_exact" {
                    self.ins(I::I32Eqz);
                }
                self.test_jump(lab(0))?;
            }
            "is_eq" | "is_ne" => {
                self.src(&a[1])?;
                self.src(&a[2])?;
                self.ins(I::Call(self.env.rt("rt_eq")));
                if ins.op == "is_ne" {
                    self.ins(I::I32Eqz);
                }
                self.test_jump(lab(0))?;
            }
            "is_lt" | "is_ge" => {
                let (x, y) = (a[1].clone(), a[2].clone());
                self.src(&x)?;
                self.ins(I::LocalSet(self.l_tmp));
                self.src(&y)?;
                self.ins(I::LocalSet(self.l_tmp + 1));
                // both small: compare tagged words directly
                self.ins(I::LocalGet(self.l_tmp));
                self.ins(I::LocalGet(self.l_tmp + 1));
                self.ins(I::I64And);
                self.ins(I::I64Const(0xF));
                self.ins(I::I64And);
                self.ins(I::I64Const(0xF));
                self.ins(I::I64Eq);
                self.ins(I::If(BlockType::Result(ValType::I32)));
                self.ins(I::LocalGet(self.l_tmp));
                self.ins(I::LocalGet(self.l_tmp + 1));
                self.ins(I::I64LtS);
                self.ins(I::Else);
                self.ins(I::LocalGet(self.l_tmp));
                self.ins(I::LocalGet(self.l_tmp + 1));
                self.ins(I::Call(self.env.rt("rt_cmp")));
                self.ins(I::I32Const(0));
                self.ins(I::I32LtS);
                self.ins(I::End);
                if ins.op == "is_ge" {
                    self.ins(I::I32Eqz);
                }
                self.test_jump(lab(0))?;
            }
            "select_val" => {
                let fail = lab(1);
                let Operand::List(pairs) = &a[2] else { return Err(CgError("bad select_val".into())) };
                let pairs = pairs.clone();
                self.src(&a[0])?;
                self.ins(I::LocalSet(self.l_tmp));
                for p in pairs.chunks(2) {
                    let val = p[0].clone();
                    let l = Self::label_of(&p[1]);
                    self.ins(I::LocalGet(self.l_tmp));
                    self.src(&val)?;
                    self.ins(I::I64Eq);
                    self.br_if_label(l)?;
                }
                self.br_label(fail)?;
            }
            "select_tuple_arity" => {
                let fail = lab(1);
                let Operand::List(pairs) = &a[2] else { return Err(CgError("bad select_tuple_arity".into())) };
                let pairs = pairs.clone();
                self.src(&a[0])?;
                self.ins(I::Call(self.env.rt("rt_tuple_arity")));
                self.ins(I::LocalSet(self.l_tmpi));
                for p in pairs.chunks(2) {
                    let n = match &p[0] {
                        Operand::U(n) => *n as i32,
                        _ => return Err(CgError("bad arity".into())),
                    };
                    self.ins(I::LocalGet(self.l_tmpi));
                    self.ins(I::I32Const(n));
                    self.ins(I::I32Eq);
                    self.br_if_label(Self::label_of(&p[1]))?;
                }
                self.br_label(fail)?;
            }

            // data
            "get_list" => {
                let (s, h, t) = (a[0].clone(), a[1].clone(), a[2].clone());
                self.src(&s)?;
                self.ins(I::LocalSet(self.l_tmp));
                let tmp = self.l_tmp;
                self.dst(&h, |fx| {
                    fx.ins(I::LocalGet(tmp));
                    fx.ins(I::I32WrapI64);
                    fx.ins(I::I32Const(-1));
                    fx.ins(I::I32Add);
                    fx.ins(I::I64Load(mem64(0)));
                    Ok(())
                })?;
                // tail: address is (ptr - 1) + 8 = ptr + 7
                self.dst(&t, |fx| {
                    fx.ins(I::LocalGet(tmp));
                    fx.ins(I::I32WrapI64);
                    fx.ins(I::I64Load(mem64(7)));
                    Ok(())
                })?;
            }
            "get_hd" => {
                let s = a[0].clone();
                self.dst(&a[1].clone(), |fx| {
                    fx.src(&s)?;
                    fx.ins(I::I32WrapI64);
                    fx.ins(I::I32Const(-1));
                    fx.ins(I::I32Add);
                    fx.ins(I::I64Load(mem64(0)));
                    Ok(())
                })?;
            }
            "get_tl" => {
                let s = a[0].clone();
                self.dst(&a[1].clone(), |fx| {
                    fx.src(&s)?;
                    fx.ins(I::I32WrapI64);
                    fx.ins(I::I64Load(mem64(7)));
                    Ok(())
                })?;
            }
            "get_tuple_element" => {
                let s = a[0].clone();
                let n = u(1);
                self.dst(&a[2].clone(), |fx| {
                    fx.src(&s)?;
                    fx.ins(I::I32WrapI64);
                    // boxed tag is 2: element n at ptr - 2 + 8 + 8n
                    fx.ins(I::I64Load(mem64(6 + 8 * n)));
                    Ok(())
                })?;
            }
            "set_tuple_element" => {
                let (v, t) = (a[0].clone(), a[1].clone());
                let n = u(2);
                self.src(&t)?;
                self.ins(I::I32WrapI64);
                self.src(&v)?;
                self.ins(I::I64Store(mem64(6 + 8 * n)));
            }
            "put_list" => {
                let (h, t) = (a[0].clone(), a[1].clone());
                self.alloc(2, 0);
                self.store_word(0, 0, |fx| fx.src(&h))?;
                self.store_word(0, 1, |fx| fx.src(&t))?;
                let p = self.l_tmpi;
                self.dst(&a[2].clone(), |fx| {
                    fx.ins(I::LocalGet(p));
                    fx.ins(I::I64ExtendI32U);
                    fx.ins(I::I64Const(1));
                    fx.ins(I::I64Or);
                    Ok(())
                })?;
            }
            "put_tuple2" => {
                let Operand::List(els) = &a[1] else { return Err(CgError("bad put_tuple2".into())) };
                let els = els.clone();
                let n = els.len() as u32;
                self.alloc(n + 1, 0);
                self.store_word(0, 0, |fx| {
                    fx.ins(I::I64Const((n as i64) << 6));
                    Ok(())
                })?;
                for (i, e) in els.iter().enumerate() {
                    let e = e.clone();
                    self.store_word(0, 1 + i as u32, |fx| fx.src(&e))?;
                }
                let p = self.l_tmpi;
                self.dst(&a[0].clone(), |fx| {
                    fx.ins(I::LocalGet(p));
                    fx.ins(I::I64ExtendI32U);
                    fx.ins(I::I64Const(2));
                    fx.ins(I::I64Or);
                    Ok(())
                })?;
            }

            // errors
            "badmatch" => self.raise_error2("badmatch", &a[0].clone())?,
            "case_end" => self.raise_error2("case_clause", &a[0].clone())?,
            "try_case_end" => self.raise_error2("try_clause", &a[0].clone())?,
            "badrecord" => self.raise_error2("badrecord", &a[0].clone())?,
            "if_end" => {
                let ia = self.env.prog.atom_index["if_clause"];
                self.ins(I::I64Const(atom(ia)));
                self.ins(I::Call(self.env.rt("rt_error")));
                self.ins(I::Drop);
                self.fault()?;
            }

            // funs
            "call_fun" => {
                let n = u(0) as u32;
                self.call_fun(n, &Operand::X(n), false)?;
            }
            "call_fun2" => {
                let n = u(1) as u32;
                self.call_fun(n, &a[2].clone(), false)?;
            }
            "make_fun3" => {
                let li = u(0) as u32;
                let Operand::List(free) = &a[2] else { return Err(CgError("bad make_fun3".into())) };
                let free = free.clone();
                let prog = self.env.prog;
                let lam = &prog.modules[self.m].lambdas[li as usize];
                let user_arity = lam.arity - lam.num_free;
                let tbl = self.env.lambda_table[&(self.m, li)];
                let gid = prog.lambdas[&(self.m, li)];
                let nfree = free.len() as u32;
                self.alloc(5 + nfree, 0);
                let mod_atom = self.mod_atom;
                self.store_word(0, 0, |fx| {
                    fx.ins(I::I64Const((((4 + nfree) as i64) << 6) | (4 << 2)));
                    Ok(())
                })?;
                self.store_word(0, 1, |fx| {
                    fx.ins(I::I64Const(small(tbl as i64)));
                    Ok(())
                })?;
                self.store_word(0, 2, |fx| {
                    fx.ins(I::I64Const(small(user_arity as i64)));
                    Ok(())
                })?;
                self.store_word(0, 3, |fx| {
                    fx.ins(I::I64Const(mod_atom));
                    Ok(())
                })?;
                self.store_word(0, 4, |fx| {
                    fx.ins(I::I64Const(small(gid as i64)));
                    Ok(())
                })?;
                for (i, v) in free.iter().enumerate() {
                    let v = v.clone();
                    self.store_word(0, 5 + i as u32, |fx| fx.src(&v))?;
                }
                let p = self.l_tmpi;
                self.dst(&a[1].clone(), |fx| {
                    fx.ins(I::LocalGet(p));
                    fx.ins(I::I64ExtendI32U);
                    fx.ins(I::I64Const(2));
                    fx.ins(I::I64Or);
                    Ok(())
                })?;
            }

            // exceptions
            "try" | "catch" => {
                let y = a[0].clone();
                self.dst(&y, |fx| {
                    fx.ins(I::I64Const(0x1B)); // a catch immediate; any non-pointer works
                    Ok(())
                })?;
                self.handlers.push(lab(1));
            }
            "try_end" => {
                let y = a[0].clone();
                self.dst(&y, |fx| {
                    fx.ins(I::I64Const(NIL));
                    Ok(())
                })?;
                self.handlers.pop();
            }
            "try_case" => {
                let y = a[0].clone();
                self.dst(&y, |fx| {
                    fx.ins(I::I64Const(NIL));
                    Ok(())
                })?;
                self.handlers.pop();
                self.ins(I::I32Const(0));
                self.ins(I::LocalSet(self.l_caught));
                self.ins(I::Call(self.env.rt("rt_exc_class")));
                self.ins(I::LocalSet(0));
                self.ins(I::Call(self.env.rt("rt_exc_reason")));
                self.ins(I::LocalSet(1));
                self.ins(I::Call(self.env.rt("rt_exc_trace")));
                self.ins(I::LocalSet(2));
            }
            "catch_end" => {
                let y = a[0].clone();
                self.dst(&y, |fx| {
                    fx.ins(I::I64Const(NIL));
                    Ok(())
                })?;
                self.handlers.pop();
                self.ins(I::LocalGet(self.l_caught));
                self.ins(I::If(BlockType::Empty));
                self.ins(I::Call(self.env.rt("rt_catch_value")));
                self.ins(I::LocalSet(0));
                self.ins(I::I32Const(0));
                self.ins(I::LocalSet(self.l_caught));
                self.ins(I::End);
            }
            "raise" => {
                let (t, v) = (a[0].clone(), a[1].clone());
                self.raise_call("rt_raise_raw", &[&t, &v])?;
            }
            "raw_raise" => {
                // x0 = class, x1 = reason, x2 = raw stacktrace
                self.ins(I::LocalGet(0));
                self.ins(I::LocalGet(1));
                self.ins(I::LocalGet(2));
                self.ins(I::Call(self.env.rt("erlang:raise/3")));
                self.ins(I::Drop);
                self.fault()?;
            }
            "build_stacktrace" => {
                self.ins(I::LocalGet(0));
                self.ins(I::Call(self.env.rt("rt_build_stacktrace")));
                self.ins(I::LocalSet(0));
            }

            // floats
            "fmove" => {
                let (s, d) = (a[0].clone(), a[1].clone());
                match (s.plain(), d.plain()) {
                    (Operand::FloatReg(r), _) => {
                        let r = *r;
                        self.dst(&d, |fx| {
                            fx.ins(I::LocalGet(fx.l_fr + r));
                            fx.ins(I::Call(fx.env.rt("rt_box_float")));
                            Ok(())
                        })?;
                    }
                    (_, Operand::FloatReg(_)) => {
                        self.dst(&d, |fx| {
                            fx.src(&s)?;
                            fx.ins(I::Call(fx.env.rt("rt_float_val")));
                            Ok(())
                        })?;
                    }
                    _ => return Err(CgError("bad fmove".into())),
                }
            }
            "fconv" => {
                let (s, d) = (a[0].clone(), a[1].clone());
                self.src(&s)?;
                self.ins(I::Call(self.env.rt("rt_is_number")));
                self.ins(I::I32Eqz);
                self.ins(I::If(BlockType::Empty));
                self.extra_depth += 1;
                let ba = self.env.prog.atom_index["badarith"];
                self.ins(I::I64Const(atom(ba)));
                self.ins(I::Call(self.env.rt("rt_error")));
                self.ins(I::Drop);
                self.fault()?;
                self.extra_depth -= 1;
                self.ins(I::End);
                self.dst(&d, |fx| {
                    fx.src(&s)?;
                    fx.ins(I::Call(fx.env.rt("rt_num_to_f64")));
                    Ok(())
                })?;
            }
            "fadd" | "fsub" | "fmul" | "fdiv" => {
                let (x, y, d) = (a[1].clone(), a[2].clone(), a[3].clone());
                let op = match ins.op {
                    "fadd" => I::F64Add,
                    "fsub" => I::F64Sub,
                    "fmul" => I::F64Mul,
                    _ => I::F64Div,
                };
                self.dst(&d, |fx| {
                    fx.fsrc(&x)?;
                    fx.fsrc(&y)?;
                    fx.ins(op);
                    Ok(())
                })?;
                self.float_check(&d)?;
            }
            "fnegate" => {
                let (x, d) = (a[1].clone(), a[2].clone());
                self.dst(&d, |fx| {
                    fx.fsrc(&x)?;
                    fx.ins(I::F64Neg);
                    Ok(())
                })?;
            }

            // receive (processes: sched.rs)
            "glowc_resume_check" => {
                let b = self.cur_block;
                self.ins(I::LocalGet(0));
                self.ins(I::I64Eqz);
                self.ins(I::If(BlockType::Empty));
                self.extra_depth += 1;
                self.ins(I::GlobalGet(self.env.g_st));
                self.ins(I::I32Load(mem32(12)));
                self.ins(I::If(BlockType::Empty));
                self.resume_args(b)?;
                self.ins(I::Call(self.env.rt("rt_susp_push")));
                self.ins(I::I64Const(0));
                self.ins(I::Return);
                self.ins(I::End);
                self.fault()?;
                self.extra_depth -= 1;
                self.ins(I::End);
            }
            "wait" => {
                let b = *self.labels.get(&lab(0)).ok_or_else(|| CgError("wait to unknown label".into()))?;
                self.resume_args(b)?;
                self.ins(I::Call(self.env.rt("rt_wait")));
                self.ins(I::I64Const(0));
                self.ins(I::Return);
            }
            "wait_timeout" => {
                let t = a[1].clone();
                self.src(&t)?;
                let b = self.cur_block + 1; // the resume label right after
                self.resume_args(b)?;
                self.ins(I::Call(self.env.rt("rt_wait_timeout")));
                self.ins(I::LocalTee(self.l_tmpi));
                self.ins(I::I32Const(-1));
                self.ins(I::I32Eq);
                self.ins(I::If(BlockType::Empty));
                self.extra_depth += 1;
                self.fault()?;
                self.extra_depth -= 1;
                self.ins(I::End);
                self.ins(I::LocalGet(self.l_tmpi));
                self.ins(I::If(BlockType::Empty));
                self.ins(I::I64Const(0));
                self.ins(I::Return);
                self.ins(I::End);
            }
            "glowc_timeout_resume" => {
                self.ins(I::Call(self.env.rt("rt_take_timed_out")));
                self.ins(I::I32Eqz);
                self.br_if_label(lab(0))?;
            }
            "loop_rec" => {
                let t = self.l_tmp;
                self.ins(I::Call(self.env.rt("rt_loop_rec")));
                self.ins(I::LocalTee(t));
                self.ins(I::I64Eqz);
                self.br_if_label(lab(0))?;
                let d = a[1].clone();
                self.dst(&d, |fx| {
                    fx.ins(I::LocalGet(t));
                    Ok(())
                })?;
            }
            "loop_rec_end" => {
                self.ins(I::Call(self.env.rt("rt_loop_rec_end")));
                self.br_label(lab(0))?;
            }
            "remove_message" => self.ins(I::Call(self.env.rt("rt_remove_message"))),
            "timeout" => self.ins(I::Call(self.env.rt("rt_timeout"))),
            "send" => {
                self.ins(I::LocalGet(0));
                self.ins(I::LocalGet(1));
                self.ins(I::Call(self.env.rt("rt_send")));
                self.check()?;
                self.ins(I::LocalSet(0));
            }

            // binaries, maps, records
            "apply" | "apply_last" => {
                // x0..x(n-1) args, x(n) module, x(n+1) function
                let n = u(0) as u32;
                self.ins(I::I64Const(NIL));
                self.ins(I::LocalSet(self.l_tmp + 1));
                for i in (0..n).rev() {
                    self.alloc(2, 0);
                    let t = self.l_tmp + 1;
                    self.store_word(0, 0, |fx| {
                        fx.ins(I::LocalGet(i));
                        Ok(())
                    })?;
                    self.store_word(0, 1, |fx| {
                        fx.ins(I::LocalGet(t));
                        Ok(())
                    })?;
                    self.ins(I::LocalGet(self.l_tmpi));
                    self.ins(I::I64ExtendI32U);
                    self.ins(I::I64Const(1));
                    self.ins(I::I64Or);
                    self.ins(I::LocalSet(t));
                }
                self.ins(I::LocalGet(n));
                self.ins(I::LocalSet(0));
                self.ins(I::LocalGet(n + 1));
                self.ins(I::LocalSet(1));
                self.ins(I::LocalGet(self.l_tmp + 1));
                self.ins(I::LocalSet(2));
                let tail = if ins.op == "apply_last" { Some(u(1)) } else { None };
                self.apply_call(3, tail)?;
            }

            op if op.starts_with("bs_") || matches!(op, "put_map_assoc" | "put_map_exact" | "get_map_elements" | "has_map_fields" | "update_record" | "is_any_native_record") => {
                crate::codegen_ext::instr(self_ext(self), ins)?;
            }

            other => return Err(CgError(format!("unsupported opcode {}", other))),
        }
        Ok(())
    }

    fn fsrc(&mut self, o: &Operand) -> Result<(), CgError> {
        match o.plain() {
            Operand::FloatReg(r) => {
                self.ins(I::LocalGet(self.l_fr + r));
                Ok(())
            }
            other => Err(CgError(format!("expected float register, got {:?}", other))),
        }
    }

    fn float_check(&mut self, d: &Operand) -> Result<(), CgError> {
        // badarith if the result is not finite: x - x != 0 for inf/nan
        let Operand::FloatReg(r) = d.plain() else { return Ok(()) };
        let fr = self.l_fr + r;
        self.ins(I::LocalGet(fr));
        self.ins(I::LocalGet(fr));
        self.ins(I::F64Sub);
        self.ins(I::F64Const(0.0.into()));
        self.ins(I::F64Ne);
        self.ins(I::If(BlockType::Empty));
        self.extra_depth += 1;
        let ba = self.env.prog.atom_index["badarith"];
        self.ins(I::I64Const(atom(ba)));
        self.ins(I::Call(self.env.rt("rt_error")));
        self.ins(I::Drop);
        self.fault()?;
        self.extra_depth -= 1;
        self.ins(I::End);
        Ok(())
    }
}

/// Access for the binary/map instruction module.
pub struct Ext<'f, 'e, 'a>(&'f mut Fx<'e, 'a>);
fn self_ext<'f, 'e, 'a>(fx: &'f mut Fx<'e, 'a>) -> Ext<'f, 'e, 'a> {
    Ext(fx)
}

impl<'f, 'e, 'a> Ext<'f, 'e, 'a> {
    pub fn ins(&mut self, i: I) {
        self.0.ins(i)
    }
    pub fn src(&mut self, o: &Operand) -> Result<(), CgError> {
        self.0.src(o)
    }
    pub fn dst(&mut self, d: &Operand, val: impl FnOnce(&mut Ext) -> Result<(), CgError>) -> Result<(), CgError> {
        self.0.dst(d, |fx| val(&mut Ext(fx)))
    }
    pub fn rt(&self, name: &str) -> u32 {
        self.0.env.rt(name)
    }
    pub fn check_or_fail(&mut self, fail: u32) -> Result<(), CgError> {
        self.0.check_or_fail(fail)
    }
    pub fn test_jump(&mut self, fail: u32) -> Result<(), CgError> {
        self.0.test_jump(fail)
    }
    pub fn br_if_label(&mut self, l: u32) -> Result<(), CgError> {
        self.0.br_if_label(l)
    }
    pub fn tmp(&self, i: u32) -> u32 {
        self.0.l_tmp + i
    }
    pub fn tmpi(&self, i: u32) -> u32 {
        self.0.l_tmpi + i
    }
    pub fn atom(&self, name: &str) -> i64 {
        atom(self.0.env.prog.atom_index[name])
    }
    pub fn global_atom(&self, local: u32) -> i64 {
        atom(self.0.env.prog.global_atom(self.0.m, local))
    }
    pub fn strings(&self) -> &[u8] {
        &self.0.env.prog.modules[self.0.m].strings
    }
    pub fn add_literal(&self, etf: Vec<u8>) -> u32 {
        self.0.env.add_literal(etf)
    }
    pub fn literal(&mut self, gi: u32) {
        self.0.literal(gi)
    }
    pub fn gc_check(&mut self, live: u32) {
        self.0.gc_check(live)
    }
    pub fn nx(&self) -> u32 {
        self.0.nx
    }
    pub fn nparams(&self) -> u32 {
        self.0.nparams
    }
    pub fn atom_name(&self, local: u32) -> &str {
        self.0.env.prog.modules[self.0.m].atom(local)
    }
    pub fn module_literal(&self, i: u32) -> &[u8] {
        &self.0.env.prog.modules[self.0.m].literals[i as usize]
    }
    pub fn g_xr(&self) -> u32 {
        self.0.env.g_xr
    }
    pub fn bs(&self, i: u32) -> u32 {
        self.0.l_bs + i
    }
    pub fn alloc_words(&mut self, words: u32) -> u32 {
        self.0.alloc(words, 0);
        self.0.l_tmpi
    }
}

fn big_etf_i64(v: i64) -> Vec<u8> {
    let neg = v < 0;
    let mut m = v.unsigned_abs();
    let mut bytes = Vec::new();
    while m > 0 {
        bytes.push((m & 0xFF) as u8);
        m >>= 8;
    }
    let mut out = vec![131, 110, bytes.len() as u8, neg as u8];
    out.extend(bytes);
    out
}

/// Big-endian two's complement bytes -> SMALL/LARGE_BIG_EXT.
fn big_etf_bytes(be: &[u8]) -> Vec<u8> {
    let neg = be[0] & 0x80 != 0;
    let mut mag: Vec<u8> = be.to_vec();
    if neg {
        // negate two's complement
        for b in mag.iter_mut() {
            *b = !*b;
        }
        for b in mag.iter_mut().rev() {
            let (r, c) = b.overflowing_add(1);
            *b = r;
            if !c {
                break;
            }
        }
    }
    let mut le: Vec<u8> = mag.into_iter().rev().collect();
    while le.last() == Some(&0) {
        le.pop();
    }
    let mut out = if le.len() < 256 {
        vec![131, 110, le.len() as u8, neg as u8]
    } else {
        let mut v = vec![131, 111];
        v.extend((le.len() as u32).to_be_bytes());
        v.push(neg as u8);
        v
    };
    out.extend(le);
    out
}
