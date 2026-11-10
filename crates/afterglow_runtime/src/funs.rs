//! Fun objects and the export table.
//!
//! Fun layout: [header(4 + nfree, FUN), small(table index), small(arity),
//! module atom, name atom (external fun) | small(lambda id) (local fun), free vars...].
//! The table index points at a wrapper in the generated module's function table with
//! signature (args..., fun) -> term. A negative index means "undefined function".

use crate::atoms::FxMap;
use crate::mem::halloc;
use crate::term::*;

static mut EXPORTS: Option<FxMap<(u32, u32, u32), i32>> = None;

fn exports() -> &'static mut FxMap<(u32, u32, u32), i32> {
    unsafe { (*core::ptr::addr_of_mut!(EXPORTS)).get_or_insert_with(Default::default) }
}

/// Blob of little-endian u32 quadruples: module atom, function atom, arity, table index.
pub fn load(blob: &[u8]) {
    let e = exports();
    for q in blob.as_chunks::<16>().0 {
        let w = |i: usize| u32::from_le_bytes([q[i], q[i + 1], q[i + 2], q[i + 3]]);
        e.insert((w(0), w(4), w(8)), w(12) as i32);
    }
}

pub fn lookup(m: Term, f: Term, a: u32) -> i32 {
    *exports().get(&(atom_index(m), atom_index(f), a)).unwrap_or(&-1)
}

pub unsafe fn export_fun(m: Term, f: Term, a: u32) -> Option<Term> {
    let idx = lookup(m, f, a);
    let p = halloc(5);
    *p = header(4, SUB_FUN);
    *p.add(1) = small(idx as i64);
    *p.add(2) = small(a as i64);
    *p.add(3) = m;
    *p.add(4) = f;
    Some(make_boxed(p))
}

#[no_mangle]
pub extern "C" fn rt_export_lookup(m: Term, f: Term, a: u32) -> i32 {
    if !is_atom(m) || !is_atom(f) {
        return -1;
    }
    lookup(m, f, a)
}

pub unsafe fn fun_arity(f: Term) -> u32 {
    small_val(*ptr(f).add(2)) as u32
}
pub unsafe fn fun_module(f: Term) -> Term {
    *ptr(f).add(3)
}
pub unsafe fn fun_name(f: Term) -> Term {
    *ptr(f).add(4)
}
pub unsafe fn fun_free(f: Term) -> Vec<Term> {
    let n = hdr_arity(*ptr(f)) as usize - 4;
    (0..n).map(|i| *ptr(f).add(5 + i)).collect()
}
pub unsafe fn is_external(f: Term) -> bool {
    is_atom(fun_name(f))
}
