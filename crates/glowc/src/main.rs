use std::process::exit;

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
        _ => {
            eprintln!("usage: glowc dump <file.beam>...");
            exit(2);
        }
    }
}
