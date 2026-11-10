//! Whole-program view: finds .beam files, walks reachability from the entry, and
//! numbers atoms, literals, functions and fun-table entries globally.

use beam_reader::{Instr, Module, Operand};
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
    /// Implemented by afterglow_runtime (export name "m:f/a").
    Runtime(String),
    /// Not available: raises undef at run time.
    Undef,
}

pub struct Program {
    /// modules whose exports all stay reachable (callback modules reached only through apply)
    pub keep: Vec<String>,
    pub search: Vec<PathBuf>,
    /// Module names that must not come from a given directory (--skip MODULE@DIR).
    pub skip: Vec<(String, String)>,
    pub modules: Vec<Module>,
    pub module_index: HashMap<String, usize>,
    pub missing_modules: HashSet<String>,
    /// afterglow_runtime function exports: name -> arity.
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
        for name in include_str!("../../afterglow_runtime/src/atoms.txt").lines() {
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

    /// Whether a .beam for `name` is available, without loading it.
    pub fn has_module(&self, name: &str) -> bool {
        self.module_index.contains_key(name)
            || !self.missing_modules.contains(name)
                && self.search.iter().any(|dir| {
                    !self.skip.iter().any(|(m, d)| m == name && dir.to_string_lossy().contains(d.as_str()))
                        && dir.join(format!("{}.beam", name)).exists()
                })
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
        // The export table names every target by atom. A target reached only through a
        // literal (`fun erlang:abs/1`) may name atoms that no loaded atom table has.
        self.atom(&mfa.0);
        self.atom(&mfa.1);
        let name = mfa_str(mfa);
        let t = if self.runtime.contains_key(&name) {
            Target::Runtime(name.clone())
        } else if let Some(m) = self.load_module(&mfa.0) {
            match self.find_function(m, &mfa.1, mfa.2) {
                Some(fi) if self.is_nif_stub(m, fi) => {
                    self.warnings.push(format!("NIF/BIF not implemented by afterglow_runtime: {}", name));
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
        let mut kept: HashSet<String> = HashSet::new();
        for name in self.keep.clone() {
            if !self.keep_exports(&name, &mut kept, &mut queue) {
                return Err(format!("--keep {}: module not found", name));
            }
        }
        // OTP calls into these through apply (sys and proc_lib callbacks, gen:init_it,
        // gen_server's own callbacks in supervisor): once reached, keep all exports.
        const OTP_DYNAMIC: [&str; 5] = ["gen", "gen_server", "gen_statem", "gen_event", "supervisor"];
        let mut pending: Vec<String> = Vec::new();
        loop {
            self.drain(&mut queue, &mut pending);
            for &(m, _) in &self.functions {
                let n = self.modules[m].name();
                if OTP_DYNAMIC.contains(&n) && !kept.contains(n) {
                    pending.push(n.to_string());
                }
            }
            let mut added = false;
            for name in std::mem::take(&mut pending) {
                if !kept.contains(&name) && self.keep_exports(&name, &mut kept, &mut queue) {
                    added = true;
                }
            }
            if !added {
                break;
            }
        }
        self.number_literals();
        Ok(())
    }

    /// Resolves every export of module `name` (once). False if there is no such module.
    fn keep_exports(&mut self, name: &str, kept: &mut HashSet<String>, queue: &mut VecDeque<(usize, usize)>) -> bool {
        let Some(km) = self.load_module(name) else { return false };
        kept.insert(name.to_string());
        let ex: Vec<Mfa> = self.modules[km]
            .exports
            .iter()
            .filter_map(|e| self.modules[km].functions.iter().find(|f| f.entry == e.label))
            .map(|f| (name.to_string(), self.modules[km].atom(f.name).to_string(), f.arity))
            .collect();
        for mfa in ex {
            self.resolve(&mfa, queue);
        }
        true
    }

    /// The atoms moved into x0 and x1 in the dozen instructions before `code[k]`.
    fn atoms_moved_before(&self, m: usize, code: &[Instr], k: usize) -> [Option<String>; 2] {
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
        mf
    }

    /// The module-name-shaped string literal moved into x0 just before `code[k]`.
    fn literal_moved_to_x0(&self, m: usize, code: &[Instr], k: usize) -> Option<String> {
        for prev in code[..k].iter().rev().take(12) {
            if prev.op == "label" || prev.op.starts_with("call") {
                break;
            }
            if prev.op == "move" {
                if let (Operand::Literal(i), Operand::X(0)) = (prev.args[0].plain(), prev.args[1].plain()) {
                    return literal_module_name(&self.modules[m].literals[*i as usize]);
                }
            }
        }
        None
    }

    fn drain(&mut self, queue: &mut VecDeque<(usize, usize)>, pending: &mut Vec<String>) {
        while let Some((m, fi)) = queue.pop_front() {
            let code = self.modules[m].functions[fi].code.clone();
            for (k, ins) in code.iter().enumerate() {
                for mfa in self.call_targets(m, ins) {
                    self.resolve(&mfa, queue);
                    // spawn(M, F, A) / apply(M, F, A) / proc_lib:start_link(M, F, A, ...)
                    // with M and F moved in as atoms just before: keep every exported M:F.
                    let dynamic = mfa.0 == "erlang" && mfa.2 == 3 && matches!(mfa.1.as_str(), "spawn" | "spawn_link" | "spawn_monitor" | "apply")
                        || mfa.0 == "erlang" && mfa.2 == 4 && mfa.1 == "spawn_opt"
                        || mfa.0 == "proc_lib" && mfa.2 == 3 && matches!(mfa.1.as_str(), "spawn" | "spawn_link")
                        || mfa.0 == "proc_lib" && (3..=5).contains(&mfa.2)
                            && matches!(mfa.1.as_str(), "start" | "start_link" | "start_monitor" | "spawn_opt");
                    // gen_server:start_link(Mod, ...) / supervisor:start_link({local, N}, Mod, Args):
                    // the callback module is called through apply; keep all of it.
                    let behaviour_start = matches!(mfa.0.as_str(), "gen_server" | "gen_statem" | "gen_event" | "supervisor")
                        && matches!(mfa.1.as_str(), "start" | "start_link" | "start_monitor");
                    // binary_to_atom("gleam@otp@static_supervisor") on a literal naming a
                    // module (Gleam's atom.create), later handed to OTP as a callback
                    // module: keep that module's exports.
                    let makes_atom = mfa.0 == "erlang"
                        && matches!(mfa.1.as_str(), "binary_to_atom" | "binary_to_existing_atom" | "list_to_atom" | "list_to_existing_atom");
                    if makes_atom {
                        if let Some(name) = self.literal_moved_to_x0(m, &code, k) {
                            if self.has_module(&name) {
                                pending.push(name);
                            }
                        }
                    }
                    if behaviour_start {
                        for name in self.atoms_moved_before(m, &code, k).into_iter().flatten() {
                            pending.push(name);
                        }
                    }
                    if dynamic {
                        let mf = self.atoms_moved_before(m, &code, k);
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
                                    self.resolve(&(mn.clone(), name, a), queue);
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
                                self.enqueue(m, t, queue);
                            }
                        }
                    }
                    "make_fun3" => {
                        if let Some(Operand::U(li)) = ins.args.first() {
                            let lam = self.modules[m].lambdas[*li as usize].clone();
                            if let Some(t) = self.function_by_label(m, lam.label) {
                                self.enqueue(m, t, queue);
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
                            self.resolve(&mfa, queue);
                        }
                    }
                }
            }
        }
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

/// The text of a literal that is a single binary shaped like a module name
/// ([a-z][a-zA-Z0-9_@]*), e.g. Gleam's "gleam@otp@static_supervisor".
pub fn literal_module_name(etf: &[u8]) -> Option<String> {
    let b = if etf.first() == Some(&131) { &etf[1..] } else { etf };
    if b.first() != Some(&109) || b.len() < 5 {
        return None;
    }
    let n = u32::from_be_bytes([b[1], b[2], b[3], b[4]]) as usize;
    let s = std::str::from_utf8(b.get(5..5 + n)?).ok()?;
    if b.len() != 5 + n || n > 255 || !s.starts_with(|c: char| c.is_ascii_lowercase())
        || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '@')
    {
        return None;
    }
    Some(s.to_string())
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
            let read_atom = |j: &mut usize| -> Option<String> {
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
