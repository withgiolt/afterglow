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
        "usage:\n  glowc dump <file.beam>...\n  glowc build --rt glowrt.wasm --beams DIR [--beams DIR]... [--skip MOD@DIRPART]... --entry M:F/0 -o out.wasm"
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
            let (mut rt, mut beams, mut skip, mut entry, mut out, mut verbose) = (None, Vec::new(), Vec::new(), None, None, false);
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
                    "-o" => out = Some(v()),
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
                eprintln!("glowc: {}", e);
                exit(1)
            });
            let runtime: HashMap<String, u32> = rt_funcs.iter().map(|f| (f.name.clone(), f.params.len() as u32)).collect();
            let mut prog = program::Program::new(beams, runtime);
            prog.skip = skip;
            if let Err(e) = prog.walk(&entry) {
                eprintln!("glowc: {}", e);
                exit(1);
            }
            if verbose {
                for w in &prog.warnings {
                    eprintln!("warning: {}", w);
                }
            } else if !prog.warnings.is_empty() {
                eprintln!("glowc: {} warnings (-v to list)", prog.warnings.len());
            }
            match module::build(&prog, &rt_funcs, &entry) {
                Ok(o) => {
                    std::fs::write(&out, &o.wasm).expect("write");
                    eprintln!("glowc: {} -> {} ({} bytes)", o.stats, out, o.wasm.len());
                }
                Err(e) => {
                    eprintln!("glowc: {}", e);
                    exit(1);
                }
            }
        }
        _ => usage(),
    }
}
