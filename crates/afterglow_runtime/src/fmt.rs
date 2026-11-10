//! Printing terms roughly like `~p`, for error messages and logs.

use crate::atoms;
use crate::build;
use crate::num;
use crate::term::*;

pub fn show(t: Term) -> String {
    let mut s = String::new();
    unsafe { write(&mut s, t, 0) };
    s
}

fn atom_needs_quotes(n: &str) -> bool {
    let mut c = n.chars();
    match c.next() {
        Some(f) if f.is_ascii_lowercase() => {}
        _ => return true,
    }
    !n.chars().all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '@')
}

unsafe fn write(s: &mut String, t: Term, depth: usize) {
    if depth > 30 || s.len() > 4000 {
        s.push_str("...");
        return;
    }
    if t == NON_VALUE {
        s.push_str("<non-value>");
    } else if is_small(t) {
        s.push_str(&small_val(t).to_string());
    } else if is_atom(t) {
        let n = atoms::name(t);
        if atom_needs_quotes(n) {
            s.push('\'');
            s.push_str(n);
            s.push('\'');
        } else {
            s.push_str(n);
        }
    } else if t == NIL {
        s.push_str("[]");
    } else if is_list(t) {
        s.push('[');
        let mut l = t;
        let mut first = true;
        while is_list(l) {
            if !first {
                s.push(',');
            }
            first = false;
            write(s, head(l), depth + 1);
            l = tail(l);
        }
        if l != NIL {
            s.push('|');
            write(s, l, depth + 1);
        }
        s.push(']');
    } else if is_pid(t) {
        s.push_str(&format!("<{}.{}.0>", pid_node_idx(t), pid_val(t)));
    } else {
        match boxed_sub(t) {
            Some(SUB_TUPLE) => {
                s.push('{');
                for i in 0..tuple_arity(t) {
                    if i > 0 {
                        s.push(',');
                    }
                    write(s, tuple_get(t, i), depth + 1);
                }
                s.push('}');
            }
            Some(SUB_FLOAT) => s.push_str(&format!("{:?}", float_val(t))),
            Some(SUB_POS_BIG) | Some(SUB_NEG_BIG) => s.push_str(&num::to_big(t).unwrap().to_string()),
            Some(SUB_BIN) => {
                let b = bin_of(t);
                let v = b.to_vec();
                s.push_str("<<");
                if b.bits % 8 == 0 && v.iter().all(|c| (32..127).contains(c)) {
                    s.push('"');
                    s.push_str(&String::from_utf8_lossy(&v[..v.len().min(400)]));
                    s.push('"');
                } else {
                    let parts: Vec<String> = v.iter().take(100).map(|c| c.to_string()).collect();
                    s.push_str(&parts.join(","));
                }
                s.push_str(">>");
            }
            Some(SUB_MAP) => {
                s.push_str("#{");
                for i in 0..build::map_size(t) {
                    if i > 0 {
                        s.push(',');
                    }
                    write(s, build::map_key(t, i), depth + 1);
                    s.push_str(" => ");
                    write(s, build::map_val(t, i), depth + 1);
                }
                s.push('}');
            }
            Some(SUB_FUN) => {
                let name = *ptr(t).add(4);
                if is_atom(name) {
                    s.push_str(&format!(
                        "fun {}:{}/{}",
                        atoms::name(*ptr(t).add(3)),
                        atoms::name(name),
                        small_val(*ptr(t).add(2))
                    ));
                } else {
                    s.push_str(&format!("#Fun<{}.{}>", atoms::name(*ptr(t).add(3)), small_val(name)));
                }
            }
            Some(SUB_REF) => s.push_str(&format!("#Ref<0.{}>", *ptr(t).add(1))),
            _ => s.push_str(&format!("<term {:#x}>", t)),
        }
    }
}
