#![allow(dead_code)]
mod codegen;
mod codegen_ext;
mod module;
mod program;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::exit;

fn usage() -> ! {
    eprintln!(
        "usage:\n  afterglow dump <file.beam>...\n  afterglow build --rt afterglow_runtime.wasm --beams DIR [--beams DIR]... [--skip MOD@DIRPART]... [--keep MOD]... --entry M:F/0 -o out.wasm"
    );
    exit(2);
}

fn parse_mfa(s: &str) -> program::Mfa {
    let (m, rest) = s.split_once(':').unwrap_or_else(|| usage());
    let (f, a) = rest.rsplit_once('/').unwrap_or_else(|| usage());
    (m.to_string(), f.to_string(), a.parse().unwrap_or_else(|_| usage()))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("dump") => {
            for path in &args[2..] {
                let data = std::fs::read(path).expect("read");
                match beam_reader::read(&data) {
                    Ok(m) => {
                        for f in &m.functions {
                            let n = f.code.iter().filter(|i| !matches!(i.op, "label" | "line" | "executable_line")).count();
                            println!("{}:{}/{} {}", m.name(), m.atom(f.name), f.arity, n);
                        }
                    }
                    Err(e) => {
                        eprintln!("{}: {}", path, e);
                        exit(1);
                    }
                }
            }
        }
        Some("build") => {
            let (mut rt, mut beams, mut skip, mut entry, mut out, mut verbose, mut list_ops) = (None, Vec::new(), Vec::new(), None, None, false, false);
            let mut keep: Vec<String> = Vec::new();
            let mut i = 2;
            while i < args.len() {
                let v = || args.get(i + 1).cloned().unwrap_or_else(|| usage());
                match args[i].as_str() {
                    "--rt" => rt = Some(v()),
                    "--beams" => beams.push(PathBuf::from(v())),
                    "--skip" => {
                        let s = v();
                        let (m, d) = s.split_once('@').unwrap_or_else(|| usage());
                        skip.push((m.to_string(), d.to_string()));
                    }
                    "--entry" => entry = Some(parse_mfa(&v())),
                    "--keep" => keep.push(v()),
                    "-o" => out = Some(v()),
                    "--list-ops" => {
                        list_ops = true;
                        i += 1;
                        continue;
                    }
                    "-v" => {
                        verbose = true;
                        i += 1;
                        continue;
                    }
                    _ => usage(),
                }
                i += 2;
            }
            let (Some(rt), Some(entry), Some(out)) = (rt, entry, out) else { usage() };
            let rt_wasm = std::fs::read(&rt).expect("read runtime");
            let rt_funcs = module::read_runtime(&rt_wasm).unwrap_or_else(|e| {
                eprintln!("afterglow: {}", e);
                exit(1)
            });
            let runtime: HashMap<String, u32> = rt_funcs.iter().map(|f| (f.name.clone(), f.params.len() as u32)).collect();
            let mut prog = program::Program::new(beams, runtime);
            prog.skip = skip;
            prog.keep = keep;
            if let Err(e) = prog.walk(&entry) {
                eprintln!("afterglow: {}", e);
                exit(1);
            }
            if verbose {
                for w in &prog.warnings {
                    eprintln!("warning: {}", w);
                }
            } else if !prog.warnings.is_empty() {
                eprintln!("afterglow: {} warnings (-v to list)", prog.warnings.len());
            }
            if list_ops {
                let mut ops: std::collections::BTreeMap<String, usize> = Default::default();
                for &(m, fi) in &prog.functions {
                    for ins in &prog.modules[m].functions[fi].code {
                        let key = match ins.op {
                            "bs_match" => match ins.args.get(2) {
                                Some(beam_reader::Operand::List(cmds)) => {
                                    for c in cmds {
                                        if let beam_reader::Operand::Atom(a) = c {
                                            *ops.entry(format!("bs_match:{}", prog.modules[m].atom(*a))).or_default() += 1;
                                        }
                                    }
                                    "bs_match".to_string()
                                }
                                _ => "bs_match".to_string(),
                            },
                            op => op.to_string(),
                        };
                        *ops.entry(key).or_default() += 1;
                    }
                }
                for (k, v) in ops {
                    println!("{} {}", k, v);
                }
                if let Ok(sample) = std::env::var("AFTERGLOW_SAMPLE") {
                    let want: Vec<&str> = sample.split(',').collect();
                    let mut seen: HashMap<&str, usize> = HashMap::new();
                    for &(m, fi) in &prog.functions {
                        for ins in &prog.modules[m].functions[fi].code {
                            if want.contains(&ins.op) {
                                let c = seen.entry(ins.op).or_default();
                                if *c < 4 {
                                    *c += 1;
                                    println!("{} {:?}", program::mfa_str(&prog.mfa_of(m, fi)), ins);
                                }
                            }
                        }
                    }
                }
                return;
            }
            match module::build(&prog, &rt_funcs, &entry) {
                Ok(o) => {
                    std::fs::write(&out, &o.wasm).expect("write");
                    eprintln!("afterglow: {} -> {} ({} bytes)", o.stats, out, o.wasm.len());
                }
                Err(e) => {
                    eprintln!("afterglow: {}", e);
                    exit(1);
                }
            }
        }
        _ => usage(),
    }
}
