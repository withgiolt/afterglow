//! Assembles the generated Wasm module: imports every glowrt function export and the
//! memory, defines the compiled functions, fun wrappers (a funcref table), apply
//! dispatch, `init` and `main`.

use crate::codegen::{self, Env};
use crate::program::{Mfa, Program, Target};
use std::collections::{BTreeMap, HashMap};
use wasm_encoder::*;

pub struct RtFunc {
    pub name: String,
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

/// Reads the function exports of glowrt.wasm with their signatures.
pub fn read_runtime(wasm: &[u8]) -> Result<Vec<RtFunc>, String> {
    use wasmparser::{Parser, Payload, TypeRef};
    let mut types: Vec<(Vec<ValType>, Vec<ValType>)> = Vec::new();
    let mut func_types: Vec<u32> = Vec::new();
    let mut out = Vec::new();
    let conv = |t: &wasmparser::ValType| match t {
        wasmparser::ValType::I32 => ValType::I32,
        wasmparser::ValType::I64 => ValType::I64,
        wasmparser::ValType::F32 => ValType::F32,
        wasmparser::ValType::F64 => ValType::F64,
        other => panic!("unsupported runtime type {:?}", other),
    };
    for payload in Parser::new(0).parse_all(wasm) {
        match payload.map_err(|e| e.to_string())? {
            Payload::TypeSection(r) => {
                for rg in r {
                    for st in rg.map_err(|e| e.to_string())?.into_types() {
                        if let wasmparser::CompositeInnerType::Func(ft) = &st.composite_type.inner {
                            types.push((ft.params().iter().map(conv).collect(), ft.results().iter().map(conv).collect()));
                        } else {
                            types.push((vec![], vec![]));
                        }
                    }
                }
            }
            Payload::ImportSection(r) => {
                for imp in r {
                    if let TypeRef::Func(t) = imp.map_err(|e| e.to_string())?.ty {
                        func_types.push(t);
                    }
                }
            }
            Payload::FunctionSection(r) => {
                for t in r {
                    func_types.push(t.map_err(|e| e.to_string())?);
                }
            }
            Payload::ExportSection(r) => {
                for e in r {
                    let e = e.map_err(|e| e.to_string())?;
                    if e.kind == wasmparser::ExternalKind::Func {
                        let (p, res) = types[func_types[e.index as usize] as usize].clone();
                        out.push(RtFunc { name: e.name.to_string(), params: p, results: res });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

struct Types {
    list: Vec<(Vec<ValType>, Vec<ValType>)>,
    index: HashMap<(Vec<ValType>, Vec<ValType>), u32>,
}
impl Types {
    fn get(&mut self, p: Vec<ValType>, r: Vec<ValType>) -> u32 {
        let k = (p, r);
        if let Some(&i) = self.index.get(&k) {
            return i;
        }
        let i = self.list.len() as u32;
        self.list.push(k.clone());
        self.index.insert(k, i);
        i
    }
    fn arity(&mut self, n: u32) -> u32 {
        self.get(vec![ValType::I64; n as usize], vec![ValType::I64])
    }
}

pub struct Output {
    pub wasm: Vec<u8>,
    pub stats: String,
}

pub fn build(prog: &Program, rt_funcs: &[RtFunc], entry: &Mfa) -> Result<Output, String> {
    let mut types = Types { list: Vec::new(), index: HashMap::new() };
    // Pre-create arity types 0..=MAX so the closure below can be pure.
    let max_arity = prog
        .functions
        .iter()
        .map(|&(m, fi)| prog.modules[m].functions[fi].arity)
        .max()
        .unwrap_or(0)
        .max(8)
        + 2;
    let arity_types: Vec<u32> = (0..=max_arity + 1).map(|n| types.arity(n)).collect();

    // Imports
    let mut imports = ImportSection::new();
    imports.import("glowrt", "memory", MemoryType { minimum: 0, maximum: None, memory64: false, shared: false, page_size_log2: None });
    let mut rt: HashMap<String, u32> = HashMap::new();
    for (i, f) in rt_funcs.iter().enumerate() {
        let t = types.get(f.params.clone(), f.results.clone());
        imports.import("glowrt", &f.name, EntityType::Function(t));
        rt.insert(f.name.clone(), i as u32);
    }
    let nimp = rt_funcs.len() as u32;

    // Defined function layout
    let f_alloc = nimp;
    let f_apply3 = nimp + 1;
    let f_apply2 = nimp + 2;
    let f_init = nimp + 3;
    let f_main = nimp + 4;
    let func_base = nimp + 5;
    rt.insert("glowc_apply3".into(), f_apply3);
    rt.insert("glowc_apply2".into(), f_apply2);
    let ncompiled = prog.functions.len() as u32;

    // Table: lambda wrappers, then export wrappers.
    let mut lambda_table: HashMap<(usize, u32), u32> = HashMap::new();
    let mut wrappers: Vec<Wrapper> = Vec::new();
    for (&(m, li), _) in &prog.lambdas {
        lambda_table.insert((m, li), wrappers.len() as u32);
        wrappers.push(Wrapper::Lambda(m, li));
    }
    let mut export_entries: Vec<(Mfa, u32)> = Vec::new();
    let sorted: BTreeMap<&Mfa, &Target> = prog.targets.iter().collect();
    for (mfa, t) in sorted {
        let include = match t {
            Target::Compiled(m, fi) => {
                let module = &prog.modules[*m];
                let f = &module.functions[*fi];
                module.exports.iter().any(|e| e.label == f.entry)
            }
            Target::Runtime(_) => true,
            Target::Undef => false,
        };
        if include {
            export_entries.push((mfa.clone(), wrappers.len() as u32));
            wrappers.push(Wrapper::Export(mfa.clone(), t.clone()));
        }
    }
    let wrapper_base = func_base + ncompiled;

    let env = Env {
        prog,
        rt: &rt,
        func_base,
        type_of_arity: &|n| arity_types[n as usize],
        g_sp: 0,
        g_st: 1,
        g_xr: 2,
        g_lit: 3,
        g_sb: 4,
        lambda_table: &lambda_table,
        extra_literals: std::cell::RefCell::new(Vec::new()),
        literal_count: prog.literals.len() as u32,
        f_alloc,
    };

    // Compile
    let mut bodies: Vec<Function> = Vec::new();
    let mut errors = Vec::new();
    for &(m, fi) in &prog.functions {
        match codegen::compile_function(&env, m, fi) {
            Ok(f) => bodies.push(f),
            Err(e) => {
                errors.push(e.0);
                let mut f = Function::new(vec![]);
                f.instruction(&Instruction::Unreachable);
                f.instruction(&Instruction::End);
                bodies.push(f);
            }
        }
    }
    if !errors.is_empty() {
        let mut uniq: Vec<String> = errors.iter().map(|e| e.split(" in ").next().unwrap().to_string()).collect();
        uniq.sort();
        uniq.dedup();
        return Err(format!("{} functions failed to compile:\n  {}\nfirst errors:\n  {}", errors.len(), uniq.join("\n  "), errors.iter().take(10).cloned().collect::<Vec<_>>().join("\n  ")));
    }

    // Data blobs
    let mut atoms_blob = Vec::new();
    for a in &prog.atoms {
        atoms_blob.extend_from_slice(a.as_bytes());
        atoms_blob.push(0);
    }
    let mut exports_blob = Vec::new();
    for (mfa, ti) in &export_entries {
        for w in [prog.atom_index[&mfa.0], prog.atom_index[&mfa.1], mfa.2, *ti] {
            exports_blob.extend_from_slice(&w.to_le_bytes());
        }
    }
    let mut lits_blob = Vec::new();
    for l in prog.literals.iter().chain(env.extra_literals.borrow().iter()) {
        lits_blob.extend_from_slice(&(l.len() as u32).to_le_bytes());
        lits_blob.extend_from_slice(l);
    }

    // Function section
    let mut funcs = FunctionSection::new();
    let t_alloc = types.get(vec![ValType::I32], vec![ValType::I32]);
    funcs.function(t_alloc);
    funcs.function(arity_types[3]);
    funcs.function(arity_types[2]);
    let t_unit = types.get(vec![], vec![]);
    funcs.function(t_unit);
    funcs.function(arity_types[0]);
    for &(m, fi) in &prog.functions {
        funcs.function(arity_types[prog.modules[m].functions[fi].arity as usize]);
    }
    let mut wrapper_bodies = Vec::new();
    for w in &wrappers {
        let (user_arity, body) = wrapper_body(prog, &env, w, func_base)?;
        funcs.function(arity_types[user_arity as usize + 1]);
        wrapper_bodies.push(body);
    }

    // Helper bodies
    let st = env.g_st;
    let mut alloc = Function::new(vec![(1, ValType::I32)]);
    {
        use Instruction as I;
        let m32 = |o| MemArg { offset: o, align: 2, memory_index: 0 };
        for i in [
            I::GlobalGet(st),
            I::I32Load(m32(4)),
            I::LocalTee(1),
            I::LocalGet(0),
            I::I32Const(3),
            I::I32Shl,
            I::I32Add,
            I::LocalTee(0), // reuse param as new hp... keep words? not needed after
            I::GlobalGet(st),
            I::I32Load(m32(8)),
            I::I32GtU,
            I::If(BlockType::Empty),
            // slow path: words = (new_hp - hp) / 8
            I::LocalGet(0),
            I::LocalGet(1),
            I::I32Sub,
            I::I32Const(3),
            I::I32ShrU,
            I::Call(rt["rt_alloc_slow"]),
            I::Return,
            I::End,
            I::GlobalGet(st),
            I::LocalGet(0),
            I::I32Store(m32(4)),
            I::LocalGet(1),
            I::End,
        ] {
            alloc.instruction(&i);
        }
    }

    let apply3 = apply_body(&rt, &arity_types, max_arity, true);
    let apply2 = apply_body(&rt, &arity_types, max_arity, false);

    let mut init = Function::new(vec![(3, ValType::I32)]);
    {
        use Instruction as I;
        let blobs = [&atoms_blob, &exports_blob, &lits_blob];
        for (k, b) in blobs.iter().enumerate() {
            init.instruction(&I::I32Const(b.len() as i32));
            init.instruction(&I::Call(rt["rt_static_alloc"]));
            init.instruction(&I::LocalTee(k as u32));
            init.instruction(&I::I32Const(0));
            init.instruction(&I::I32Const(b.len() as i32));
            init.instruction(&I::MemoryInit { mem: 0, data_index: k as u32 });
            init.instruction(&I::DataDrop(k as u32));
        }
        for (k, b) in blobs.iter().enumerate() {
            init.instruction(&I::LocalGet(k as u32));
            init.instruction(&I::I32Const(b.len() as i32));
        }
        init.instruction(&I::Call(rt["rt_init"]));
        init.instruction(&I::GlobalSet(env.g_lit));
        init.instruction(&I::Call(rt["rt_state"]));
        init.instruction(&I::GlobalSet(env.g_st));
        init.instruction(&I::Call(rt["rt_xregs"]));
        init.instruction(&I::GlobalSet(env.g_xr));
        init.instruction(&I::Call(rt["rt_stack_top"]));
        init.instruction(&I::GlobalSet(env.g_sp));
        init.instruction(&I::Call(rt["rt_stack_base"]));
        init.instruction(&I::GlobalSet(env.g_sb));
        init.instruction(&I::End);
    }

    let entry_id = match prog.targets.get(entry) {
        Some(Target::Compiled(m, fi)) => prog.function_id[&(*m, *fi)],
        _ => return Err("entry not compiled".into()),
    };
    if entry.2 != 0 {
        return Err("entry must have arity 0".into());
    }
    let mut main = Function::new(vec![]);
    main.instruction(&Instruction::Call(rt["rt_stack_top"]));
    main.instruction(&Instruction::GlobalSet(env.g_sp));
    main.instruction(&Instruction::Call(func_base + entry_id));
    main.instruction(&Instruction::End);

    // Assemble
    let mut module = Module::new();
    let mut ts = TypeSection::new();
    for (p, r) in &types.list {
        ts.ty().function(p.clone(), r.clone());
    }
    module.section(&ts);
    module.section(&imports);
    module.section(&funcs);
    let mut tables = TableSection::new();
    tables.table(TableType {
        element_type: RefType::FUNCREF,
        minimum: wrappers.len() as u64,
        maximum: Some(wrappers.len() as u64),
        table64: false,
        shared: false,
    });
    module.section(&tables);
    let mut globals = GlobalSection::new();
    for _ in 0..5 {
        globals.global(GlobalType { val_type: ValType::I32, mutable: true, shared: false }, &ConstExpr::i32_const(0));
    }
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("init", ExportKind::Func, f_init);
    exports.export("main", ExportKind::Func, f_main);
    module.section(&exports);
    let mut elems = ElementSection::new();
    let idxs: Vec<u32> = (0..wrappers.len() as u32).map(|i| wrapper_base + i).collect();
    elems.active(Some(0), &ConstExpr::i32_const(0), Elements::Functions(idxs.into()));
    module.section(&elems);
    module.section(&DataCountSection { count: 3 });
    let mut code = CodeSection::new();
    code.function(&alloc);
    code.function(&apply3);
    code.function(&apply2);
    code.function(&init);
    code.function(&main);
    for b in &bodies {
        code.function(b);
    }
    for b in &wrapper_bodies {
        code.function(b);
    }
    module.section(&code);
    let mut data = DataSection::new();
    data.passive(atoms_blob.iter().copied());
    data.passive(exports_blob.iter().copied());
    data.passive(lits_blob.iter().copied());
    module.section(&data);

    let stats = format!(
        "{} functions from {} modules, {} wrappers ({} lambdas), {} atoms, {} literals ({} bytes)",
        ncompiled,
        prog.modules.len(),
        wrappers.len(),
        lambda_table.len(),
        prog.atoms.len(),
        prog.literals.len() + env.extra_literals.borrow().len(),
        lits_blob.len()
    );
    Ok(Output { wasm: module.finish(), stats })
}

enum Wrapper {
    Lambda(usize, u32),
    Export(Mfa, Target),
}

/// (args..., fun) -> target(args..., free...)
fn wrapper_body(prog: &Program, env: &Env, w: &Wrapper, func_base: u32) -> Result<(u32, Function), String> {
    use Instruction as I;
    let mut f = Function::new(vec![]);
    match w {
        Wrapper::Lambda(m, li) => {
            let lam = &prog.modules[*m].lambdas[*li as usize];
            let user = lam.arity - lam.num_free;
            let fi = prog.function_by_label(*m, lam.label).ok_or("lambda target missing")?;
            for i in 0..user {
                f.instruction(&I::LocalGet(i));
            }
            for k in 0..lam.num_free {
                f.instruction(&I::LocalGet(user));
                f.instruction(&I::I32WrapI64);
                f.instruction(&I::I64Load(MemArg { offset: 8 * (5 + k as u64) - 2, align: 3, memory_index: 0 }));
            }
            f.instruction(&I::ReturnCall(func_base + prog.function_id[&(*m, fi)]));
            f.instruction(&I::End);
            Ok((user, f))
        }
        Wrapper::Export(mfa, t) => {
            for i in 0..mfa.2 {
                f.instruction(&I::LocalGet(i));
            }
            let idx = match t {
                Target::Compiled(m, fi) => func_base + prog.function_id[&(*m, *fi)],
                Target::Runtime(name) => env.rt[name],
                Target::Undef => unreachable!(),
            };
            f.instruction(&I::ReturnCall(idx));
            f.instruction(&I::End);
            Ok((mfa.2, f))
        }
    }
}

/// apply/3 (M, F, Args) or apply/2 (Fun, Args): args go through XREGS, then a
/// br_table on the argument count picks the call_indirect type.
fn apply_body(rt: &HashMap<String, u32>, arity_types: &[u32], max_arity: u32, three: bool) -> Function {
    use Instruction as I;
    // locals: params (2 or 3 i64), fun i64, n i32, idx i32
    let np = if three { 3 } else { 2 };
    let l_fun = np;
    let l_n = np + 1;
    let l_idx = np + 2;
    let mut f = Function::new(vec![(1, ValType::I64), (2, ValType::I32)]);
    let args = np - 1;
    for i in 0..np {
        f.instruction(&I::LocalGet(i));
    }
    f.instruction(&I::Call(rt[if three { "rt_apply_prepare" } else { "rt_apply_prepare_fun" }]));
    f.instruction(&I::LocalTee(l_fun));
    f.instruction(&I::I64Eqz);
    f.instruction(&I::If(BlockType::Empty));
    f.instruction(&I::I64Const(0));
    f.instruction(&I::Return);
    f.instruction(&I::End);
    f.instruction(&I::LocalGet(args));
    f.instruction(&I::Call(rt["rt_list_len"]));
    f.instruction(&I::LocalSet(l_n));
    f.instruction(&I::LocalGet(l_fun));
    f.instruction(&I::LocalGet(l_n));
    f.instruction(&I::Call(rt["rt_fun_check"]));
    f.instruction(&I::LocalTee(l_idx));
    f.instruction(&I::I32Const(0));
    f.instruction(&I::I32LtS);
    f.instruction(&I::If(BlockType::Empty));
    f.instruction(&I::I64Const(0));
    f.instruction(&I::Return);
    f.instruction(&I::End);
    let n = max_arity as usize;
    for _ in 0..=n {
        f.instruction(&I::Block(BlockType::Empty));
    }
    f.instruction(&I::LocalGet(l_n));
    let targets: Vec<u32> = (0..=n as u32).collect();
    f.instruction(&I::BrTable(targets.into(), n as u32));
    for k in 0..=n {
        f.instruction(&I::End);
        // case k
        for i in 0..k {
            f.instruction(&I::Call(rt["rt_xregs"]));
            f.instruction(&I::I64Load(MemArg { offset: 8 * i as u64, align: 3, memory_index: 0 }));
        }
        f.instruction(&I::LocalGet(l_fun));
        f.instruction(&I::LocalGet(l_idx));
        f.instruction(&I::ReturnCallIndirect { type_index: arity_types[k + 1], table_index: 0 });
    }
    f.instruction(&I::End);
    f
}
