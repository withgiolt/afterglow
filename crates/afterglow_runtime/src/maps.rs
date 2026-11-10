//! Maps as sorted flat arrays: [header(1 + 2n, MAP), small(n), k1, v1, ...], keys in
//! map key order (`cmp::map_key_cmp`). Matches ERTS iteration order for maps of up to
//! 32 keys (flatmaps); larger ERTS maps are HAMTs with hash order.

use crate::atoms::*;
use crate::build::{self, map_key, map_size, map_val};
use crate::cmp::map_key_cmp;
use crate::exc::{badarg, error2};
use crate::term::*;
use core::cmp::Ordering;

/// Index of `k` in map `m`.
pub unsafe fn find(m: Term, k: Term) -> Option<usize> {
    let n = map_size(m);
    // Small maps: linear scan with a word compare first (atoms and smalls are common).
    if n <= 8 {
        for i in 0..n {
            let mk = map_key(m, i);
            if mk == k || (!is_immed(k) && map_key_cmp(mk, k) == Ordering::Equal) {
                return Some(i);
            }
        }
        return None;
    }
    let (mut lo, mut hi) = (0usize, n);
    while lo < hi {
        let mid = (lo + hi) / 2;
        match map_key_cmp(map_key(m, mid), k) {
            Ordering::Less => lo = mid + 1,
            Ordering::Greater => hi = mid,
            Ordering::Equal => return Some(mid),
        }
    }
    None
}

pub unsafe fn get(m: Term, k: Term) -> Option<Term> {
    find(m, k).map(|i| map_val(m, i))
}

/// Value of `k` in `m`, or 0 (no exception): get_map_elements.
#[no_mangle]
pub unsafe extern "C" fn rt_map_get(m: Term, k: Term) -> Term {
    if !is_map(m) {
        return 0;
    }
    get(m, k).unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn rt_map_has(m: Term, k: Term) -> i32 {
    (is_map(m) && find(m, k).is_some()) as i32
}

pub unsafe fn pairs(m: Term) -> Vec<(Term, Term)> {
    (0..map_size(m)).map(|i| (map_key(m, i), map_val(m, i))).collect()
}

/// Puts the pairs in `kvs` (k, v, k, v...) into `m`.
pub unsafe fn put_all(m: Term, kvs: &[(Term, Term)], exact: bool) -> Result<Term, Term> {
    if !is_map(m) {
        return Err(error2(BADMAP, m));
    }
    // Fast path: replace values of existing keys without changing the key set.
    let mut idx = Vec::with_capacity(kvs.len());
    let mut all_found = true;
    for &(k, _) in kvs {
        match find(m, k) {
            Some(i) => idx.push(Some(i)),
            None => {
                if exact {
                    return Err(error2(BADKEY, k));
                }
                all_found = false;
                idx.push(None);
            }
        }
    }
    if all_found {
        let n = map_size(m);
        let p = crate::mem::halloc(2 + 2 * n);
        core::ptr::copy_nonoverlapping(ptr(m), p, 2 + 2 * n);
        for (j, &(_, v)) in kvs.iter().enumerate() {
            *p.add(3 + 2 * idx[j].unwrap()) = v;
        }
        return Ok(make_boxed(p));
    }
    let mut v = pairs(m);
    v.extend_from_slice(kvs);
    Ok(build::map_from(v))
}

/// put_map_assoc / put_map_exact: `n` pairs at `scratch` (k, v words).
#[no_mangle]
pub unsafe extern "C" fn rt_map_put(m: Term, n: u32, scratch: u32, exact: i32) -> Term {
    let s = core::slice::from_raw_parts(scratch as usize as *const u64, 2 * n as usize);
    let kvs: Vec<(Term, Term)> = s.chunks(2).map(|c| (c[0], c[1])).collect();
    match put_all(m, &kvs, exact != 0) {
        Ok(t) => t,
        Err(e) => e,
    }
}

macro_rules! bif {
    ($name:literal, fn $f:ident($($a:ident),*) $body:block) => {
        #[export_name = $name]
        pub unsafe extern "C" fn $f($($a: Term),*) -> Term $body
    };
}

bif!("erlang:map_size/1", fn map_size_(m) {
    if !is_map(m) { return error2(BADMAP, m); }
    small(map_size(m) as i64)
});
bif!("erlang:map_get/2", fn map_get(k, m) {
    if !is_map(m) { return error2(BADMAP, m); }
    match get(m, k) { Some(v) => v, None => error2(BADKEY, k) }
});
bif!("maps:get/2", fn maps_get(k, m) { map_get(k, m) });
bif!("maps:get/3", fn maps_get3(k, m, d) {
    if !is_map(m) { return error2(BADMAP, m); }
    get(m, k).unwrap_or(d)
});
bif!("erlang:is_map_key/2", fn is_map_key(k, m) {
    if !is_map(m) { return error2(BADMAP, m); }
    boolean(find(m, k).is_some())
});
bif!("maps:is_key/2", fn maps_is_key(k, m) { is_map_key(k, m) });
bif!("maps:find/2", fn maps_find(k, m) {
    if !is_map(m) { return error2(BADMAP, m); }
    match get(m, k) { Some(v) => build::tuple(&[OK, v]), None => ERROR }
});
bif!("maps:put/3", fn maps_put(k, v, m) {
    match put_all(m, &[(k, v)], false) { Ok(t) => t, Err(e) => e }
});
bif!("maps:update/3", fn maps_update(k, v, m) {
    match put_all(m, &[(k, v)], true) { Ok(t) => t, Err(e) => e }
});
bif!("maps:remove/2", fn maps_remove(k, m) {
    if !is_map(m) { return error2(BADMAP, m); }
    match find(m, k) {
        None => m,
        Some(i) => {
            let mut v = pairs(m);
            v.remove(i);
            build::map_sorted(&v)
        }
    }
});
bif!("maps:take/2", fn maps_take(k, m) {
    if !is_map(m) { return error2(BADMAP, m); }
    match find(m, k) {
        None => ERROR,
        Some(i) => {
            let mut v = pairs(m);
            let (_, val) = v.remove(i);
            let rest = build::map_sorted(&v);
            build::tuple(&[val, rest])
        }
    }
});
bif!("maps:keys/1", fn maps_keys(m) {
    if !is_map(m) { return error2(BADMAP, m); }
    let v: Vec<Term> = (0..map_size(m)).map(|i| map_key(m, i)).collect();
    build::list(&v)
});
bif!("maps:values/1", fn maps_values(m) {
    if !is_map(m) { return error2(BADMAP, m); }
    let v: Vec<Term> = (0..map_size(m)).map(|i| map_val(m, i)).collect();
    build::list(&v)
});
bif!("maps:to_list/1", fn maps_to_list(m) {
    if !is_map(m) { return error2(BADMAP, m); }
    let v: Vec<Term> = (0..map_size(m)).map(|i| build::tuple(&[map_key(m, i), map_val(m, i)])).collect();
    build::list(&v)
});
bif!("maps:from_list/1", fn maps_from_list(l) {
    let Some(items) = build::list_to_vec(l) else { return badarg() };
    let mut v = Vec::with_capacity(items.len());
    for it in items {
        if !is_tuple(it) || tuple_arity(it) != 2 { return badarg(); }
        v.push((tuple_get(it, 0), tuple_get(it, 1)));
    }
    build::map_from(v)
});
bif!("maps:from_keys/2", fn maps_from_keys(l, val) {
    let Some(items) = build::list_to_vec(l) else { return badarg() };
    build::map_from(items.into_iter().map(|k| (k, val)).collect())
});
bif!("maps:merge/2", fn maps_merge(a, b) {
    if !is_map(a) { return error2(BADMAP, a); }
    if !is_map(b) { return error2(BADMAP, b); }
    let mut v = pairs(a);
    v.extend(pairs(b));
    build::map_from(v)
});
bif!("maps:new/0", fn maps_new() { build::map_sorted(&[]) });

// erts_internal:map_next/3 (Iter, Map, Acc), behind maps:next/1, to_list/1, fold/3.
// Like ERTS on flatmaps: with Acc = `iterator` it returns the whole chain eagerly,
// {K1, V1, {K2, V2, ... none}}; otherwise [{K, V}, ... | Acc].
bif!("erts_internal:map_next/3", fn map_next(it, m, acc) {
    if !is_map(m) || !is_small(it) { return badarg(); }
    let i = small_val(it) as usize;
    let n = map_size(m);
    if acc == ITERATOR {
        let mut out = NONE;
        for j in (i..n).rev() {
            out = build::tuple(&[map_key(m, j), map_val(m, j), out]);
        }
        return out;
    }
    let mut out = acc;
    for j in (i..n).rev() {
        out = build::cons(build::tuple(&[map_key(m, j), map_val(m, j)]), out);
    }
    out
});
