//! Whole-program view: finds .beam files, walks reachability from the entry, and
//! numbers atoms, literals, functions and fun-table entries globally.

use beam_reader::{Module, Operand};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;

pub type Mfa = (String, String, u32);

pub fn mfa_str(m: &Mfa) -> String {
    format!("{}:{}/{}", m.0, m.1, m.2)
}

/// How a call target is provided.
#[derive(Clone, Debug)]
pub enum Target {
    /// Compiled from .beam: (module index, function index in module).
    Compiled(usize, usize),
    /// Implemented by glowrt (export name "m:f/a").
    Runtime(String),
    /// Not available: raises undef at run time.
    Undef,
}

pub struct Program {
    /// modules whose exports all stay reachable (callback modules reached only through apply)
    pub keep: Vec<String>,
    pub search: Vec<PathBuf>,
    /// Module names that must not come from a given directory (glowvm's AtomVM shims).
    pub skip: Vec<(String, String)>,
    pub modules: Vec<Module>,
    pub module_index: HashMap<String, usize>,
    pub missing_modules: HashSet<String>,
    /// glowrt function exports: name -> arity.
    pub runtime: HashMap<String, u32>,
    pub atoms: Vec<String>,
    pub atom_index: HashMap<String, u32>,
    /// Reachable compiled functions in order: (module, function index).
    pub functions: Vec<(usize, usize)>,
    pub function_id: HashMap<(usize, usize), u32>,
    pub targets: HashMap<Mfa, Target>,
    /// Per module: global literal index of each local literal.
    /// Global literal index of each (module, local literal) used by reachable code.
    pub lit_index: HashMap<(usize, u32), u32>,
    pub literals: Vec<Vec<u8>>,
    /// Lambdas used by make_fun3: (module, FunT index) -> global lambda id.
    pub lambdas: BTreeMap<(usize, u32), u32>,
    /// External funs referenced (literals, make_fun) or callable through apply.
    pub undef: Vec<Mfa>,
    pub warnings: Vec<String>,
}

impl Program {
    pub fn new(search: Vec<PathBuf>, runtime: HashMap<String, u32>) -> Self {
        let mut p = Program {
            search,
            skip: Vec::new(),
            modules: Vec::new(),
            module_index: HashMap::new(),
            missing_modules: HashSet::new(),
            runtime,
            atoms: Vec::new(),
            atom_index: HashMap::new(),
            functions: Vec::new(),
            function_id: HashMap::new(),
            keep: Vec::new(),
            targets: HashMap::new(),
            lit_index: HashMap::new(),
            literals: Vec::new(),
            lambdas: BTreeMap::new(),
            undef: Vec::new(),
            warnings: Vec::new(),
        };
        for name in include_str!("../../glowrt/src/atoms.txt").lines() {
            p.atom(name);
        }
        p
    }

    pub fn atom(&mut self, name: &str) -> u32 {
        if let Some(&i) = self.atom_index.get(name) {
            return i;
        }
        let i = self.atoms.len() as u32;
        self.atoms.push(name.to_string());
        self.atom_index.insert(name.to_string(), i);
        i
    }

    pub fn load_module(&mut self, name: &str) -> Option<usize> {
        if let Some(&i) = self.module_index.get(name) {
            return Some(i);
        }
        if self.missing_modules.contains(name) {
            return None;
        }
        for dir in &self.search {
            let path = dir.join(format!("{}.beam", name));
            let skipped = self.skip.iter().any(|(m, d)| m == name && dir.to_string_lossy().contains(d.as_str()));
            if skipped || !path.exists() {
                continue;
            }
            let data = std::fs::read(&path).ok()?;
            let m = match beam_reader::read(&data) {
                Ok(m) => m,
                Err(e) => {
                    self.warnings.push(format!("cannot read {}: {}", path.display(), e));
                    continue;
                }
            };
            let idx = self.modules.len();
            for a in m.atoms.clone() {
                self.atom(&a);
            }
            self.modules.push(m);
            self.module_index.insert(name.to_string(), idx);
            return Some(idx);
        }
        self.missing_modules.insert(name.to_string());
        None
    }

    pub fn find_function(&self, m: usize, f: &str, a: u32) -> Option<usize> {
        let module = &self.modules[m];
        module.functions.iter().position(|func| module.atom(func.name) == f && func.arity == a)
    }

    pub fn function_by_label(&self, m: usize, label: u32) -> Option<usize> {
        self.modules[m].functions.iter().position(|f| f.entry == label)
    }

    /// A function whose body only raises nif_error is a NIF stub.
    fn is_nif_stub(&self, m: usize, fi: usize) -> bool {
        let module = &self.modules[m];
        let f = &module.functions[fi];
        f.code.iter().any(|i| i.op == "nif_start")
            || f.code.iter().any(|i| {
                matches!(i.op, "call_ext" | "call_ext_only" | "call_ext_last")
                    && matches!(i.args.get(1), Some(Operand::U(imp)) if {
                        let im = &module.imports[*imp as usize];
                        module.atom(im.module) == "erlang" && module.atom(im.function) == "nif_error"
                    })
            }) && f.code.iter().filter(|i| !matches!(i.op, "label" | "line" | "func_info" | "move" | "executable_line")).count() <= 2
    }

    /// Resolves an MFA, loading its module if needed. Queues newly compiled functions.
    pub fn resolve(&mut self, mfa: &Mfa, queue: &mut VecDeque<(usize, usize)>) -> Target {
        if let Some(t) = self.targets.get(mfa) {
            return t.clone();
        }
        let name = mfa_str(mfa);
        let t = if self.runtime.contains_key(&name) {
            Target::Runtime(name.clone())
        } else if let Some(m) = self.load_module(&mfa.0) {
            match self.find_function(m, &mfa.1, mfa.2) {
                Some(fi) if self.is_nif_stub(m, fi) => {
                    self.warnings.push(format!("NIF/BIF not implemented by glowrt: {}", name));
                    self.undef.push(mfa.clone());
                    Target::Undef
                }
                Some(fi) => {
                    self.enqueue(m, fi, queue);
                    Target::Compiled(m, fi)
                }
                None => {
                    self.warnings.push(format!("undefined function: {}", name));
                    self.undef.push(mfa.clone());
                    Target::Undef
                }
            }
        } else {
            self.warnings.push(format!("no module for {}", name));
            self.undef.push(mfa.clone());
            Target::Undef
        };
        self.targets.insert(mfa.clone(), t.clone());
        t
    }

    pub fn enqueue(&mut self, m: usize, fi: usize, queue: &mut VecDeque<(usize, usize)>) {
        if self.function_id.contains_key(&(m, fi)) {
            return;
        }
        let id = self.functions.len() as u32;
        self.functions.push((m, fi));
        self.function_id.insert((m, fi), id);
        let f = &self.modules[m].functions[fi];
        let mfa = (self.modules[m].name().to_string(), self.modules[m].atom(f.name).to_string(), f.arity);
        self.targets.insert(mfa, Target::Compiled(m, fi));
        queue.push_back((m, fi));
    }

    /// Walks everything reachable from `entry`.
    pub fn walk(&mut self, entry: &Mfa) -> Result<(), String> {
        let mut queue = VecDeque::new();
        match self.resolve(entry, &mut queue) {
            Target::Compiled(..) => {}
            _ => return Err(format!("entry {} not found", mfa_str(entry))),
        }
        for name in self.keep.clone() {
            let Some(km) = self.load_module(&name) else { return Err(format!("--keep {}: module not found", name)) };
            let ex: Vec<Mfa> = self.modules[km]
                .exports
                .iter()
                .filter_map(|e| self.modules[km].functions.iter().find(|f| f.entry == e.label))
                .map(|f| (name.clone(), self.modules[km].atom(f.name).to_string(), f.arity))
                .collect();
            for mfa in ex {
                self.resolve(&mfa, &mut queue);
            }
        }
        while let Some((m, fi)) = queue.pop_front() {
            let code = self.modules[m].functions[fi].code.clone();
            for (k, ins) in code.iter().enumerate() {
                for mfa in self.call_targets(m, ins) {
                    self.resolve(&mfa, &mut queue);
                    // spawn(M, F, A) / apply(M, F, A) with M and F moved in as atoms
                    // just before: keep every exported M:F reachable.
                    let dynamic = mfa.0 == "erlang" && mfa.2 == 3 && matches!(mfa.1.as_str(), "spawn" | "spawn_link" | "spawn_monitor" | "apply")
                        || mfa.0 == "erlang" && mfa.2 == 4 && mfa.1 == "spawn_opt"
                        || mfa.0 == "proc_lib" && mfa.2 == 3 && matches!(mfa.1.as_str(), "spawn" | "spawn_link");
                    if dynamic {
                        let mut mf: [Option<String>; 2] = [None, None];
                        for prev in code[..k].iter().rev().take(12) {
                            if prev.op == "label" || prev.op.starts_with("call") {
                                break;
                            }
                            if prev.op == "move" {
                                if let (Operand::Atom(a), Operand::X(x)) = (prev.args[0].plain(), prev.args[1].plain()) {
                                    if *x < 2 && mf[*x as usize].is_none() && *a > 0 {
                                        mf[*x as usize] = Some(self.modules[m].atom(*a).to_string());
                                    }
                                }
                            }
                        }
                        if let [Some(mn), Some(fname)] = mf {
                            if let Some(tm) = self.load_module(&mn) {
                                let ex: Vec<(String, u32)> = self.modules[tm]
                                    .exports
                                    .iter()
                                    .filter_map(|e| {
                                        let f = self.modules[tm].functions.iter().find(|f| f.entry == e.label)?;
                                        let name = self.modules[tm].atom(f.name).to_string();
                                        (name == fname).then_some((name, f.arity))
                                    })
                                    .collect();
                                for (name, a) in ex {
                                    self.resolve(&(mn.clone(), name, a), &mut queue);
                                }
                            }
                        } else {
                            self.warnings.push(format!("{} with a computed target in {}", mfa_str(&mfa), self.modules[m].name()));
                        }
                    }
                }
                match ins.op {
                    "call" | "call_only" | "call_last" => {
                        if let Some(Operand::Label(l)) = ins.args.get(1) {
                            if let Some(t) = self.function_by_label(m, *l) {
                                self.enqueue(m, t, &mut queue);
                            }
                        }
                    }
                    "make_fun3" => {
                        if let Some(Operand::U(li)) = ins.args.first() {
                            let lam = self.modules[m].lambdas[*li as usize].clone();
                            if let Some(t) = self.function_by_label(m, lam.label) {
                                self.enqueue(m, t, &mut queue);
                            }
                            let n = self.lambdas.len() as u32;
                            self.lambdas.entry((m, *li as u32)).or_insert(n);
                        }
                    }
                    _ => {}
                }
                // External funs inside literals.
                for op in &ins.args {
                    if let Operand::Literal(i) = op.plain() {
                        let lit = self.modules[m].literals[*i as usize].clone();
                        for mfa in literal_exports(&lit) {
                            self.resolve(&mfa, &mut queue);
                        }
                    }
                }
            }
        }
        self.number_literals();
        Ok(())
    }

    /// Numbers only the literals that reachable functions mention.
    fn number_literals(&mut self) {
        fn walk(o: &Operand, out: &mut Vec<u32>) {
            match o {
                Operand::Literal(i) => out.push(*i),
                Operand::TypedReg(r, _) => walk(r, out),
                Operand::List(v) => v.iter().for_each(|x| walk(x, out)),
                _ => {}
            }
        }
        for &(m, fi) in &self.functions {
            let mut used = Vec::new();
            for ins in &self.modules[m].functions[fi].code {
                for a in &ins.args {
                    walk(a, &mut used);
                }
            }
            for i in used {
                if !self.lit_index.contains_key(&(m, i)) {
                    self.lit_index.insert((m, i), self.literals.len() as u32);
                    self.literals.push(self.modules[m].literals[i as usize].clone());
                }
            }
        }
    }

    fn import_mfa(&self, m: usize, imp: u32) -> Mfa {
        let module = &self.modules[m];
        let im = &module.imports[imp as usize];
        (module.atom(im.module).to_string(), module.atom(im.function).to_string(), im.arity)
    }

    /// External call targets of one instruction.
    pub fn call_targets(&self, m: usize, ins: &beam_reader::Instr) -> Vec<Mfa> {
        let imp = |i: usize| match ins.args.get(i) {
            Some(Operand::U(x)) => Some(*x as u32),
            _ => None,
        };
        let r = match ins.op {
            "call_ext" | "call_ext_only" | "call_ext_last" => imp(1),
            "bif0" => imp(0),
            "bif1" | "bif2" | "bif3" => imp(1),
            "gc_bif1" | "gc_bif2" | "gc_bif3" => imp(2),
            _ => None,
        };
        match r {
            Some(i) => vec![self.import_mfa(m, i)],
            None => vec![],
        }
    }

    pub fn mfa_of(&self, m: usize, fi: usize) -> Mfa {
        let module = &self.modules[m];
        let f = &module.functions[fi];
        (module.name().to_string(), module.atom(f.name).to_string(), f.arity)
    }

    pub fn import_target(&self, m: usize, imp: u32) -> (Mfa, Target) {
        let mfa = self.import_mfa(m, imp);
        let t = self.targets.get(&mfa).cloned().unwrap_or(Target::Undef);
        (mfa, t)
    }

    /// Global atom index of a module-local atom.
    pub fn global_atom(&self, m: usize, local: u32) -> u32 {
        self.atom_index[self.modules[m].atom(local)]
    }
}

/// Finds EXPORT_EXT (fun M:F/A) inside an ETF literal.
pub fn literal_exports(etf: &[u8]) -> Vec<Mfa> {
    let mut out = Vec::new();
    let mut i = 0;
    // Literals are small; a byte scan for tag 113 followed by two atoms and an integer
    // is good enough, verified by decoding the atoms.
    while i < etf.len() {
        if etf[i] == 113 {
            let mut j = i + 1;
            let mut read_atom = |j: &mut usize| -> Option<String> {
                match etf.get(*j)? {
                    119 | 115 => {
                        let n = *etf.get(*j + 1)? as usize;
                        let s = std::str::from_utf8(etf.get(*j + 2..*j + 2 + n)?).ok()?.to_string();
                        *j += 2 + n;
                        Some(s)
                    }
                    118 | 100 => {
                        let n = u16::from_be_bytes([*etf.get(*j + 1)?, *etf.get(*j + 2)?]) as usize;
                        let s = std::str::from_utf8(etf.get(*j + 3..*j + 3 + n)?).ok()?.to_string();
                        *j += 3 + n;
                        Some(s)
                    }
                    _ => None,
                }
            };
            if let (Some(m), Some(f)) = (read_atom(&mut j), read_atom(&mut j)) {
                if etf.get(j) == Some(&97) {
                    if let Some(a) = etf.get(j + 1) {
                        out.push((m, f, *a as u32));
                    }
                }
            }
        }
        i += 1;
    }
    out
}
