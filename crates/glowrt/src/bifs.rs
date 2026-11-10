//! Built-in functions, exported under their Erlang names. Each takes and returns
//! terms; NON_VALUE (0) means an exception was raised.

use crate::atoms::{self, *};
use crate::build;
use crate::cmp;
use crate::etf;
use crate::exc::{self, badarg, error};
use crate::num;
use crate::roots::proc;
use crate::term::*;

macro_rules! bif {
    ($name:literal, fn $f:ident($($a:ident),*) $body:block) => {
        #[export_name = $name]
        pub unsafe extern "C" fn $f($($a: Term),*) -> Term $body
    };
}

static mut PERSISTENT: Vec<(Term, Term)> = Vec::new();
static mut UNIQUE: i64 = 0;

pub unsafe fn for_each_bif_root(_f: &mut dyn FnMut(&mut Term)) {}

pub fn reset() {}

// ---------------------------------------------------------------------------
// arithmetic and comparison

bif!("erlang:+/2", fn op_add(a, b) { num::add_fast(a, b) });
bif!("erlang:-/2", fn op_sub(a, b) {
    if is_small(a) && is_small(b) { return build::int(small_val(a) - small_val(b)); }
    num::sub(a, b)
});
bif!("erlang:*/2", fn op_mul(a, b) {
    if is_small(a) && is_small(b) {
        if let Some(r) = small_val(a).checked_mul(small_val(b)) { return build::int(r); }
    }
    num::mul(a, b)
});
bif!("erlang://2", fn op_fdiv(a, b) { num::fdiv(a, b) });
bif!("erlang:div/2", fn op_div(a, b) { num::div(a, b) });
bif!("erlang:rem/2", fn op_rem(a, b) { num::rem(a, b) });
bif!("erlang:band/2", fn op_band(a, b) { num::band(a, b) });
bif!("erlang:bor/2", fn op_bor(a, b) { num::bor(a, b) });
bif!("erlang:bxor/2", fn op_bxor(a, b) { num::bxor(a, b) });
bif!("erlang:bsl/2", fn op_bsl(a, b) { num::bsl(a, b) });
bif!("erlang:bsr/2", fn op_bsr(a, b) { num::bsr(a, b) });
bif!("erlang:bnot/1", fn op_bnot(a) { num::bnot(a) });
bif!("erlang:-/1", fn op_neg(a) { num::neg(a) });
bif!("erlang:+/1", fn op_plus(a) { if is_number(a) { a } else { error(BADARITH) } });
bif!("erlang:abs/1", fn bif_abs(a) { num::abs(a) });
bif!("erlang:float/1", fn bif_float(a) { num::float_of(a) });
bif!("erlang:trunc/1", fn bif_trunc(a) { num::to_int(a, f64::trunc) });
bif!("erlang:round/1", fn bif_round(a) { num::to_int(a, f64::round) });
bif!("erlang:floor/1", fn bif_floor(a) { num::to_int(a, f64::floor) });
bif!("erlang:ceil/1", fn bif_ceil(a) { num::to_int(a, f64::ceil) });
bif!("erlang:max/2", fn bif_max(a, b) { if cmp::compare(a, b) == core::cmp::Ordering::Less { b } else { a } });
bif!("erlang:min/2", fn bif_min(a, b) { if cmp::compare(a, b) == core::cmp::Ordering::Greater { b } else { a } });

bif!("erlang:==/2", fn op_eq(a, b) { boolean(cmp::eq(a, b)) });
bif!("erlang:/=/2", fn op_ne(a, b) { boolean(!cmp::eq(a, b)) });
bif!("erlang:=:=/2", fn op_eqx(a, b) { boolean(cmp::exact_eq(a, b)) });
bif!("erlang:=/=/2", fn op_nex(a, b) { boolean(!cmp::exact_eq(a, b)) });
bif!("erlang:</2", fn op_lt(a, b) { boolean(cmp::compare(a, b).is_lt()) });
bif!("erlang:>/2", fn op_gt(a, b) { boolean(cmp::compare(a, b).is_gt()) });
bif!("erlang:=</2", fn op_le(a, b) { boolean(cmp::compare(a, b).is_le()) });
bif!("erlang:>=/2", fn op_ge(a, b) { boolean(cmp::compare(a, b).is_ge()) });
bif!("erts_internal:cmp_term/2", fn cmp_term(a, b) { small(cmp::rt_cmp(a, b) as i64) });

unsafe fn bool_of(t: Term) -> Option<bool> {
    if t == TRUE { Some(true) } else if t == FALSE { Some(false) } else { None }
}
bif!("erlang:and/2", fn op_and(a, b) {
    match (bool_of(a), bool_of(b)) { (Some(x), Some(y)) => boolean(x && y), _ => badarg() }
});
bif!("erlang:or/2", fn op_or(a, b) {
    match (bool_of(a), bool_of(b)) { (Some(x), Some(y)) => boolean(x || y), _ => badarg() }
});
bif!("erlang:xor/2", fn op_xor(a, b) {
    match (bool_of(a), bool_of(b)) { (Some(x), Some(y)) => boolean(x != y), _ => badarg() }
});
bif!("erlang:not/1", fn op_not(a) { match bool_of(a) { Some(x) => boolean(!x), None => badarg() } });

// ---------------------------------------------------------------------------
// type tests

bif!("erlang:is_atom/1", fn is_atom_(a) { boolean(is_atom(a)) });
bif!("erlang:is_integer/1", fn is_integer_(a) { boolean(is_integer(a)) });
bif!("erlang:is_integer/3", fn is_integer3(a, lo, hi) {
    if !is_integer(lo) || !is_integer(hi) { return crate::exc::badarg(); }
    boolean(is_integer(a) && cmp::compare(lo, a) != core::cmp::Ordering::Greater && cmp::compare(a, hi) != core::cmp::Ordering::Greater)
});
bif!("erlang:is_float/1", fn is_float_(a) { boolean(is_float(a)) });
bif!("erlang:is_number/1", fn is_number_(a) { boolean(is_number(a)) });
bif!("erlang:is_list/1", fn is_list_(a) { boolean(a == NIL || is_list(a)) });
bif!("erlang:is_tuple/1", fn is_tuple_(a) { boolean(is_tuple(a)) });
bif!("erlang:is_binary/1", fn is_binary_(a) { boolean(is_binary(a)) });
bif!("erlang:is_bitstring/1", fn is_bitstring_(a) { boolean(is_bitstring(a)) });
bif!("erlang:is_map/1", fn is_map_(a) { boolean(is_map(a)) });
bif!("erlang:is_pid/1", fn is_pid_(a) { boolean(is_pid(a)) });
bif!("erlang:is_port/1", fn is_port_(a) { boolean(is_port(a)) });
bif!("erlang:is_reference/1", fn is_reference_(a) { boolean(is_ref(a)) });
bif!("erlang:is_boolean/1", fn is_boolean_(a) { boolean(a == TRUE || a == FALSE) });
bif!("erlang:is_function/1", fn is_function_(a) { boolean(is_fun(a)) });
bif!("erlang:is_function/2", fn is_function2_(a, n) {
    if !is_small(n) || small_val(n) < 0 { return badarg(); }
    boolean(is_fun(a) && crate::funs::fun_arity(a) as i64 == small_val(n))
});
bif!("erlang:is_record/2", fn is_record2(t, tag) {
    boolean(is_tuple(t) && tuple_arity(t) > 0 && tuple_get(t, 0) == tag && is_atom(tag))
});
bif!("erlang:is_record/3", fn is_record3(t, tag, n) {
    boolean(is_tuple(t) && is_small(n) && tuple_arity(t) as i64 == small_val(n) && tuple_arity(t) > 0 && tuple_get(t, 0) == tag)
});

// ---------------------------------------------------------------------------
// tuples, lists

bif!("erlang:element/2", fn element(n, t) {
    if is_small(n) && is_tuple(t) {
        let i = small_val(n);
        if i >= 1 && i as usize <= tuple_arity(t) { return tuple_get(t, i as usize - 1); }
    }
    badarg()
});
bif!("erlang:setelement/3", fn setelement(n, t, v) {
    if is_small(n) && is_tuple(t) {
        let i = small_val(n);
        let a = tuple_arity(t);
        if i >= 1 && i as usize <= a {
            let mut els: Vec<Term> = (0..a).map(|j| tuple_get(t, j)).collect();
            els[i as usize - 1] = v;
            return build::tuple(&els);
        }
    }
    badarg()
});
bif!("erlang:tuple_size/1", fn tuple_size(t) { if is_tuple(t) { small(tuple_arity(t) as i64) } else { badarg() } });
bif!("erlang:size/1", fn size(t) {
    if is_tuple(t) { small(tuple_arity(t) as i64) }
    else if is_binary_obj(t) { small((bin_of(t).bits / 8) as i64) }
    else { badarg() }
});
bif!("erlang:make_tuple/2", fn make_tuple(n, v) {
    if !is_small(n) || small_val(n) < 0 { return badarg(); }
    build::tuple(&vec![v; small_val(n) as usize])
});
bif!("erlang:make_tuple/3", fn make_tuple3(n, d, inits) {
    if !is_small(n) || small_val(n) < 0 { return badarg(); }
    let mut v = vec![d; small_val(n) as usize];
    let Some(items) = build::list_to_vec(inits) else { return badarg() };
    for it in items {
        if !is_tuple(it) || tuple_arity(it) != 2 { return badarg(); }
        let (i, x) = (tuple_get(it, 0), tuple_get(it, 1));
        if !is_small(i) || small_val(i) < 1 || small_val(i) as usize > v.len() { return badarg(); }
        v[small_val(i) as usize - 1] = x;
    }
    build::tuple(&v)
});
bif!("erlang:append_element/2", fn append_element(t, x) {
    if !is_tuple(t) { return badarg(); }
    let mut v: Vec<Term> = (0..tuple_arity(t)).map(|j| tuple_get(t, j)).collect();
    v.push(x);
    build::tuple(&v)
});
bif!("erlang:tuple_to_list/1", fn tuple_to_list(t) {
    if !is_tuple(t) { return badarg(); }
    let v: Vec<Term> = (0..tuple_arity(t)).map(|j| tuple_get(t, j)).collect();
    build::list(&v)
});
bif!("erlang:list_to_tuple/1", fn list_to_tuple(l) {
    match build::list_to_vec(l) { Some(v) => build::tuple(&v), None => badarg() }
});
bif!("erlang:hd/1", fn hd(l) { if is_list(l) { head(l) } else { badarg() } });
bif!("erlang:tl/1", fn tl(l) { if is_list(l) { tail(l) } else { badarg() } });
bif!("erlang:length/1", fn length(l) {
    let mut n = 0i64;
    let mut x = l;
    while is_list(x) { n += 1; x = tail(x); }
    if x == NIL { small(n) } else { badarg() }
});
bif!("erlang:++/2", fn append(a, b) {
    if a == NIL { return b; }
    match build::list_to_vec(a) { Some(v) => build::list_with_tail(&v, b), None => badarg() }
});
bif!("erlang:--/2", fn subtract(a, b) {
    let (Some(mut xs), Some(ys)) = (build::list_to_vec(a), build::list_to_vec(b)) else { return badarg() };
    for y in ys {
        if let Some(i) = xs.iter().position(|x| cmp::exact_eq(*x, y)) { xs.remove(i); }
    }
    build::list(&xs)
});
bif!("lists:reverse/2", fn reverse2(l, tail_) {
    let mut acc = tail_;
    let mut x = l;
    while is_list(x) { acc = build::cons(head(x), acc); x = tail(x); }
    if x == NIL { acc } else { badarg() }
});
bif!("lists:reverse/1", fn reverse1(l) { reverse2(l, NIL) });
bif!("lists:member/2", fn member(e, l) {
    let mut x = l;
    while is_list(x) { if cmp::exact_eq(head(x), e) { return TRUE; } x = tail(x); }
    if x == NIL { FALSE } else { badarg() }
});
unsafe fn keyfind_(k: Term, n: Term, l: Term) -> Option<Term> {
    if !is_small(n) || small_val(n) < 1 { return None; }
    let i = small_val(n) as usize - 1;
    let mut x = l;
    while is_list(x) {
        let t = head(x);
        if is_tuple(t) && tuple_arity(t) > i && cmp::eq(tuple_get(t, i), k) { return Some(t); }
        x = tail(x);
    }
    None
}
bif!("lists:keyfind/3", fn keyfind(k, n, l) {
    if !is_small(n) || small_val(n) < 1 { return badarg(); }
    keyfind_(k, n, l).unwrap_or(FALSE)
});
bif!("lists:keymember/3", fn keymember(k, n, l) {
    if !is_small(n) || small_val(n) < 1 { return badarg(); }
    boolean(keyfind_(k, n, l).is_some())
});
bif!("lists:keysearch/3", fn keysearch(k, n, l) {
    if !is_small(n) || small_val(n) < 1 { return badarg(); }
    match keyfind_(k, n, l) { Some(t) => build::tuple(&[atoms::intern("value"), t]), None => FALSE }
});

// ---------------------------------------------------------------------------
// exceptions

bif!("erlang:error/1", fn error1(r) { error(r) });
bif!("erlang:error/2", fn error2_(r, _a) { error(r) });
bif!("erlang:error/3", fn error3(r, _a, _o) { error(r) });
bif!("erlang:exit/1", fn exit1(r) { exc::raise(EXIT, r) });
bif!("erlang:throw/1", fn throw1(r) { exc::raise(THROW, r) });
bif!("erlang:raise/3", fn raise3(c, r, t) { exc::raise3(c, r, t) });
bif!("erlang:nif_error/1", fn nif_error1(r) { error(r) });
bif!("erlang:nif_error/2", fn nif_error2(r, _a) { error(r) });

// ---------------------------------------------------------------------------
// process-ish

bif!("erlang:group_leader/0", fn group_leader() { pid(1) });
bif!("erlang:make_ref/0", fn make_ref() {
    let p = proc();
    p.refs += 1;
    make_ref_with(p.refs)
});
pub unsafe fn make_ref_with(id: u64) -> Term {
    let p = crate::mem::halloc(2);
    *p = header(1, SUB_REF);
    *p.add(1) = id;
    make_boxed(p)
}
bif!("erlang:unique_integer/0", fn unique0() { UNIQUE += 1; small(UNIQUE) });
bif!("erlang:unique_integer/1", fn unique1(_o) { UNIQUE += 1; small(UNIQUE) });

unsafe fn pdict_find(k: Term) -> Option<Term> {
    let mut x = proc().pdict;
    while is_list(x) {
        let kv = head(x);
        if cmp::exact_eq(tuple_get(kv, 0), k) { return Some(kv); }
        x = tail(x);
    }
    None
}
unsafe fn pdict_remove(k: Term) -> Term {
    let items = build::list_to_vec(proc().pdict).unwrap_or_default();
    let mut old = UNDEFINED;
    let rest: Vec<Term> = items.into_iter().filter(|kv| {
        if cmp::exact_eq(tuple_get(*kv, 0), k) { old = tuple_get(*kv, 1); false } else { true }
    }).collect();
    proc().pdict = build::list(&rest);
    old
}
bif!("erlang:get/1", fn get1(k) { pdict_find(k).map(|kv| tuple_get(kv, 1)).unwrap_or(UNDEFINED) });
bif!("erlang:get/0", fn get0() { proc().pdict });
bif!("erlang:put/2", fn put2(k, v) {
    let old = pdict_remove(k);
    let kv = build::tuple(&[k, v]);
    proc().pdict = build::cons(kv, proc().pdict);
    old
});
bif!("erlang:erase/1", fn erase1(k) { pdict_remove(k) });

bif!("persistent_term:get/1", fn pt_get1(k) {
    let v = &*core::ptr::addr_of!(PERSISTENT);
    for (kk, vv) in v { if cmp::exact_eq(*kk, k) { return *vv; } }
    badarg()
});
bif!("persistent_term:get/2", fn pt_get2(k, d) {
    let v = &*core::ptr::addr_of!(PERSISTENT);
    for (kk, vv) in v { if cmp::exact_eq(*kk, k) { return *vv; } }
    d
});
bif!("persistent_term:put/2", fn pt_put(k, v) {
    // Stored terms must outlive the request: copy them into the permanent area.
    let kb = etf::term_to_binary(k);
    let vb = etf::term_to_binary(v);
    let (Some(kb), Some(vb)) = (kb, vb) else { return badarg() };
    crate::mem::TO_PERM = true;
    let kp = etf::binary_to_term(&kb, false).unwrap();
    let vp = etf::binary_to_term(&vb, false).unwrap();
    crate::mem::TO_PERM = false;
    let store = &mut *core::ptr::addr_of_mut!(PERSISTENT);
    store.retain(|(kk, _)| !cmp::exact_eq(*kk, kp));
    store.push((kp, vp));
    OK
});

// ---------------------------------------------------------------------------
// conversions

bif!("erlang:atom_to_binary/1", fn atom_to_binary1(a) { if is_atom(a) { build::str_bin(atoms::name(a)) } else { badarg() } });
bif!("erlang:atom_to_binary/2", fn atom_to_binary2(a, _e) { atom_to_binary1(a) });
bif!("erlang:atom_to_list/1", fn atom_to_list(a) {
    if !is_atom(a) { return badarg(); }
    let v: Vec<Term> = atoms::name(a).chars().map(|c| small(c as i64)).collect();
    build::list(&v)
});
unsafe fn bin_text(b: Term) -> Option<String> {
    if !is_binary(b) { return None; }
    String::from_utf8(bin_of(b).to_vec()).ok()
}
bif!("erlang:binary_to_atom/2", fn binary_to_atom2(b, _e) { match bin_text(b) { Some(s) => atoms::intern(&s), None => badarg() } });
bif!("erlang:binary_to_atom/1", fn binary_to_atom1(b) { binary_to_atom2(b, UTF8) });
bif!("erlang:binary_to_existing_atom/2", fn binary_to_existing_atom2(b, _e) {
    match bin_text(b).and_then(|s| atoms::lookup(&s)) { Some(a) => a, None => badarg() }
});
bif!("erlang:binary_to_existing_atom/1", fn binary_to_existing_atom1(b) { binary_to_existing_atom2(b, UTF8) });
pub unsafe fn char_list(l: Term) -> Option<String> {
    let v = build::list_to_vec(l)?;
    let mut s = String::new();
    for c in v {
        if !is_small(c) { return None; }
        s.push(char::from_u32(small_val(c) as u32)?);
    }
    Some(s)
}
bif!("erlang:list_to_atom/1", fn list_to_atom(l) { match char_list(l) { Some(s) => atoms::intern(&s), None => badarg() } });
bif!("erlang:list_to_existing_atom/1", fn list_to_existing_atom(l) {
    match char_list(l).and_then(|s| atoms::lookup(&s)) { Some(a) => a, None => badarg() }
});

unsafe fn int_to_string(i: Term, base: Term) -> Option<String> {
    let b = if is_small(base) { small_val(base) } else { return None };
    if !(2..=36).contains(&b) { return None; }
    let v = num::to_big(i)?;
    Some(v.to_str_radix(b as u32).to_uppercase())
}
bif!("erlang:integer_to_binary/1", fn integer_to_binary1(i) {
    if is_small(i) { return build::str_bin(&small_val(i).to_string()); }
    match int_to_string(i, small(10)) { Some(s) => build::str_bin(&s), None => badarg() }
});
bif!("erlang:integer_to_binary/2", fn integer_to_binary2(i, b) {
    match int_to_string(i, b) { Some(s) => build::str_bin(&s), None => badarg() }
});
unsafe fn str_list(s: &str) -> Term {
    let v: Vec<Term> = s.chars().map(|c| small(c as i64)).collect();
    build::list(&v)
}
bif!("erlang:integer_to_list/1", fn integer_to_list1(i) {
    match int_to_string(i, small(10)) { Some(s) => str_list(&s), None => badarg() }
});
bif!("erlang:integer_to_list/2", fn integer_to_list2(i, b) {
    match int_to_string(i, b) { Some(s) => str_list(&s), None => badarg() }
});
unsafe fn parse_int(s: &str, base: u32) -> Option<Term> {
    if s.is_empty() { return None; }
    let (neg, digits) = match s.as_bytes()[0] {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(base)) { return None; }
    let v = num_bigint::BigInt::parse_bytes(digits.as_bytes(), base)?;
    Some(build::big(&if neg { -v } else { v }))
}
bif!("erlang:binary_to_integer/1", fn binary_to_integer1(b) {
    match bin_text(b).and_then(|s| parse_int(&s, 10)) { Some(t) => t, None => badarg() }
});
bif!("erlang:binary_to_integer/2", fn binary_to_integer2(b, base) {
    if !is_small(base) { return badarg(); }
    match bin_text(b).and_then(|s| parse_int(&s, small_val(base) as u32)) { Some(t) => t, None => badarg() }
});
bif!("erts_internal:binary_to_integer/2", fn ei_binary_to_integer(b, base) {
    if !is_small(base) { return badarg(); }
    match bin_text(b).and_then(|s| parse_int(&s, small_val(base) as u32)) { Some(t) => t, None => BADARG }
});
bif!("erlang:list_to_integer/1", fn list_to_integer1(l) {
    match char_list(l).and_then(|s| parse_int(&s, 10)) { Some(t) => t, None => badarg() }
});
bif!("erlang:list_to_integer/2", fn list_to_integer2(l, base) {
    if !is_small(base) { return badarg(); }
    match char_list(l).and_then(|s| parse_int(&s, small_val(base) as u32)) { Some(t) => t, None => badarg() }
});
bif!("erts_internal:list_to_integer/2", fn ei_list_to_integer(l, base) {
    // Returns {Integer, Rest} or badarg/no_integer, as erts_internal does.
    let Some(s) = char_list(l) else { return BADARG };
    let b = small_val(base) as u32;
    let bytes: Vec<char> = s.chars().collect();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == '-' || bytes[i] == '+') { i += 1; }
    let start = i;
    while i < bytes.len() && bytes[i].is_digit(b) { i += 1; }
    if i == start { return atoms::intern("no_integer"); }
    let num: String = bytes[..i].iter().collect();
    let rest: String = bytes[i..].iter().collect();
    match parse_int(&num, b) { Some(v) => build::tuple(&[v, str_list(&rest)]), None => BADARG }
});

/// Erlang's shortest round-trip float formatting (float_to_binary(F, [short])).
/// ERTS takes the shortest round-trip digits and prints them in decimal or scientific
/// form, whichever is shorter; decimal on a tie: 2500.0 -> "2.5e3", 100.0 -> "100.0",
/// 1.0e-5 -> "1.0e-5", 0.0012 -> "0.0012".
pub fn float_short(f: f64) -> String {
    let s = format!("{:e}", f.abs()); // shortest digits: "2.5e3", "1e15", "1.5e-7"
    let (m, e) = s.split_at(s.find('e').unwrap());
    let exp: i64 = e[1..].parse().unwrap();
    let digits: String = m.chars().filter(|c| *c != '.').collect();
    let sign = if f.is_sign_negative() { "-" } else { "" };
    let frac = if digits.len() > 1 { &digits[1..] } else { "0" };
    let sci = format!("{}{}.{}e{}", sign, &digits[..1], frac, exp);
    let dec = if exp >= 0 {
        let int_len = exp as usize + 1;
        if digits.len() > int_len {
            format!("{}{}.{}", sign, &digits[..int_len], &digits[int_len..])
        } else {
            format!("{}{}{}.0", sign, digits, "0".repeat(int_len - digits.len()))
        }
    } else {
        format!("{}0.{}{}", sign, "0".repeat((-exp - 1) as usize), digits)
    };
    if sci.len() < dec.len() { sci } else { dec }
}

/// `float_to_list(F)` / `float_to_binary(F)` default: 20 significant digits, scientific.
pub fn float_scientific(f: f64, digits: usize) -> String {
    let s = format!("{:.*e}", digits, f);
    // Rust: "1.00000000000000000000e0" -> Erlang: "1.00000000000000000000e+00"
    let (m, e) = s.split_at(s.find('e').unwrap());
    let exp: i32 = e[1..].parse().unwrap();
    format!("{}e{}{:02}", m, if exp < 0 { '-' } else { '+' }, exp.abs())
}

unsafe fn float_opts(f: f64, opts: Term) -> Option<String> {
    let items = build::list_to_vec(opts)?;
    let mut out = float_scientific(f, 20);
    let mut compact = false;
    for o in &items {
        if *o == SHORT {
            out = float_short(f);
        } else if *o == COMPACT {
            compact = true;
        } else if is_tuple(*o) && tuple_arity(*o) == 2 {
            let (k, v) = (tuple_get(*o, 0), tuple_get(*o, 1));
            if !is_small(v) { return None; }
            let n = small_val(v) as usize;
            if k == DECIMALS {
                out = format!("{:.*}", n, f);
                if compact && out.contains('.') {
                    let t = out.trim_end_matches('0');
                    out = if t.ends_with('.') { format!("{}0", t) } else { t.to_string() };
                }
            } else if k == SCIENTIFIC {
                out = float_scientific(f, n);
            }
        }
    }
    Some(out)
}
bif!("erlang:float_to_binary/1", fn float_to_binary1(f) {
    if !is_float(f) { return badarg(); }
    build::str_bin(&float_scientific(float_val(f), 20))
});
bif!("erlang:float_to_binary/2", fn float_to_binary2(f, o) {
    if !is_float(f) { return badarg(); }
    match float_opts(float_val(f), o) { Some(s) => build::str_bin(&s), None => badarg() }
});
bif!("erlang:float_to_list/1", fn float_to_list1(f) {
    if !is_float(f) { return badarg(); }
    str_list(&float_scientific(float_val(f), 20))
});
bif!("erlang:float_to_list/2", fn float_to_list2(f, o) {
    if !is_float(f) { return badarg(); }
    match float_opts(float_val(f), o) { Some(s) => str_list(&s), None => badarg() }
});
fn parse_erlang_float(s: &str) -> Option<f64> {
    // Erlang requires digits on both sides of the point.
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') { i += 1; }
    let ds = i;
    while i < b.len() && b[i].is_ascii_digit() { i += 1; }
    if i == ds || i >= b.len() || b[i] != b'.' { return None; }
    i += 1;
    let fs = i;
    while i < b.len() && b[i].is_ascii_digit() { i += 1; }
    if i == fs { return None; }
    if i < b.len() {
        if b[i] != b'e' && b[i] != b'E' { return None; }
        i += 1;
        if i < b.len() && (b[i] == b'-' || b[i] == b'+') { i += 1; }
        let es = i;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        if i == es || i != b.len() { return None; }
    }
    let f: f64 = s.parse().ok()?;
    if f.is_finite() { Some(f) } else { None }
}
bif!("erlang:binary_to_float/1", fn binary_to_float(b) {
    match bin_text(b).and_then(|s| parse_erlang_float(&s)) { Some(f) => build::float(f), None => badarg() }
});
bif!("erlang:list_to_float/1", fn list_to_float(l) {
    match char_list(l).and_then(|s| parse_erlang_float(&s)) { Some(f) => build::float(f), None => badarg() }
});

bif!("erlang:term_to_binary/1", fn term_to_binary1(t) {
    match etf::term_to_binary(t) { Some(v) => build::binary(&v), None => error(NOTSUP) }
});
bif!("erlang:term_to_binary/2", fn term_to_binary2(t, _o) { term_to_binary1(t) });
bif!("erlang:binary_to_term/1", fn binary_to_term1(b) {
    if !is_binary(b) { return badarg(); }
    match etf::binary_to_term(&bin_of(b).to_vec(), false) { Some(t) => t, None => badarg() }
});
bif!("erlang:binary_to_term/2", fn binary_to_term2(b, _o) { binary_to_term1(b) });

// ---------------------------------------------------------------------------
// misc

bif!("erlang:phash2/1", fn phash2_1(t) { small((phash(t) & 0x7FF_FFFF) as i64) });
bif!("erlang:phash2/2", fn phash2_2(t, r) {
    if !is_small(r) || small_val(r) < 1 { return badarg(); }
    small((phash(t) % small_val(r) as u64) as i64)
});
unsafe fn phash(t: Term) -> u64 {
    let bytes = etf::term_to_binary(t).unwrap_or_default();
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes { h = (h ^ b as u64).wrapping_mul(0x100000001b3); }
    h & 0xFFFF_FFFF
}
bif!("erlang:display/1", fn display(t) { crate::host_log(&crate::fmt::show(t)); TRUE });

// Logging. glowrt has no logger backend (OTP's needs ETS and the application env), so
// the ?LOG_* macros (logger:macro_log, e.g. proc_lib's crash reports when a spawned
// process dies) print one line to the host log instead: "[Level] What", cut at 2 KB.
fn log_line(level: Term, what: Term, args: Option<Term>) -> Term {
    let mut s = format!("[{}] {}", crate::fmt::show(level), crate::fmt::show(what));
    if let Some(a) = args { s.push(' '); s.push_str(&crate::fmt::show(a)); }
    if s.len() > 2048 {
        let mut cut = 2048;
        while !s.is_char_boundary(cut) { cut -= 1; }
        s.truncate(cut);
        s.push_str(" ...");
    }
    crate::host_log(&s);
    atoms::intern("ok")
}
bif!("logger:macro_log/3", fn macro_log3(_loc, level, what) { log_line(level, what, None) });
bif!("logger:macro_log/4", fn macro_log4(_loc, level, what, _meta) { log_line(level, what, None) });
bif!("logger:macro_log/5", fn macro_log5(_loc, level, fmt, args, _meta) { log_line(level, fmt, Some(args)) });
bif!("error_logger:get_format_depth/0", fn format_depth() { atoms::intern("unlimited") });
// limit_term calls get_format_depth locally, which no override reaches; unlimited depth
// means the term comes back unchanged.
bif!("error_logger:limit_term/1", fn limit_term(t) { t });
// Environment: the host fills ENV_BUF with "K=V\0" entries (rt_env_buf + rt_env_set).
static mut ENV_BUF: Vec<u8> = Vec::new();
static mut ENV_VARS: Vec<(String, String)> = Vec::new();
#[no_mangle]
pub unsafe extern "C" fn rt_env_buf(len: u32) -> u32 {
    let b = &mut *core::ptr::addr_of_mut!(ENV_BUF);
    b.clear();
    b.resize(len as usize, 0);
    b.as_mut_ptr() as usize as u32
}
#[no_mangle]
pub unsafe extern "C" fn rt_env_set() {
    let b = &*core::ptr::addr_of!(ENV_BUF);
    let e = &mut *core::ptr::addr_of_mut!(ENV_VARS);
    e.clear();
    for entry in b.split(|&c| c == 0) {
        if let Some(i) = entry.iter().position(|&c| c == b'=') {
            e.push((String::from_utf8_lossy(&entry[..i]).into(), String::from_utf8_lossy(&entry[i + 1..]).into()));
        }
    }
}
unsafe fn charlist(s: &str) -> Term {
    let v: Vec<Term> = s.chars().map(|c| small(c as i64)).collect();
    build::list(&v)
}
bif!("os:getenv/1", fn getenv(n) {
    let name: String = if is_binary(n) {
        String::from_utf8_lossy(&bin_of(n).to_vec()).into()
    } else {
        let Some(v) = build::list_to_vec(n) else { return badarg() };
        let mut s = String::new();
        for c in v {
            if !is_small(c) { return badarg(); }
            match char::from_u32(small_val(c) as u32) { Some(ch) => s.push(ch), None => return badarg() }
        }
        s
    };
    let e = &*core::ptr::addr_of!(ENV_VARS);
    match e.iter().find(|(k, _)| *k == name) {
        Some((_, v)) => charlist(v),
        None => FALSE,
    }
});
bif!("os:getenv/0", fn getenv0() {
    let e = &*core::ptr::addr_of!(ENV_VARS);
    let v: Vec<Term> = e.iter().map(|(k, v)| charlist(&format!("{}={}", k, v))).collect();
    build::list(&v)
});
/// Time in `unit` from a nanosecond count. None for a bad unit.
unsafe fn in_unit(ns: i128, unit: Term) -> Option<Term> {
    let per_sec: i128 = if is_small(unit) && small_val(unit) > 0 {
        small_val(unit) as i128
    } else if is_atom(unit) {
        match atoms::name(unit) {
            "second" | "seconds" => 1,
            "millisecond" | "milli_seconds" => 1_000,
            "microsecond" | "micro_seconds" => 1_000_000,
            "nanosecond" | "nano_seconds" | "native" | "perf_counter" => 1_000_000_000,
            _ => return None,
        }
    } else {
        return None;
    };
    let v = (ns * per_sec).div_euclid(1_000_000_000);
    Some(build::int(v as i64))
}
fn now_ns() -> i128 {
    (crate::host_now_us() * 1000.0) as i128
}
bif!("os:system_time/0", fn os_system_time0() { build::int(now_ns() as i64) });
bif!("os:system_time/1", fn os_system_time(u) { in_unit(now_ns(), u).unwrap_or_else(|| badarg()) });
bif!("erlang:system_time/0", fn system_time0() { build::int(now_ns() as i64) });
bif!("erlang:system_time/1", fn system_time1(u) { in_unit(now_ns(), u).unwrap_or_else(|| badarg()) });
bif!("erlang:monotonic_time/0", fn monotonic0() { build::int(now_ns() as i64) });
bif!("erlang:monotonic_time/1", fn monotonic1(u) { in_unit(now_ns(), u).unwrap_or_else(|| badarg()) });
bif!("erlang:time_offset/0", fn time_offset0() { small(0) });
bif!("erlang:time_offset/1", fn time_offset1(_u) { small(0) });
bif!("os:timestamp/0", fn os_timestamp() {
    let us = (now_ns() / 1000) as i64;
    build::tuple(&[small(us / 1_000_000_000_000), small(us / 1_000_000 % 1_000_000), small(us % 1_000_000)])
});
bif!("erlang:timestamp/0", fn erlang_timestamp() { os_timestamp() });
bif!("erlang:convert_time_unit/3", fn convert_time_unit(t, from, to) {
    let Some(v) = crate::num::to_i64(t) else { return badarg() };
    let ns = match in_unit_inv(v as i128, from) { Some(x) => x, None => return badarg() };
    in_unit(ns, to).unwrap_or_else(|| badarg())
});
unsafe fn in_unit_inv(v: i128, unit: Term) -> Option<i128> {
    let per_sec: i128 = if is_small(unit) && small_val(unit) > 0 {
        small_val(unit) as i128
    } else if is_atom(unit) {
        match atoms::name(unit) {
            "second" | "seconds" => 1,
            "millisecond" | "milli_seconds" => 1_000,
            "microsecond" | "micro_seconds" => 1_000_000,
            "nanosecond" | "nano_seconds" | "native" | "perf_counter" => 1_000_000_000,
            _ => return None,
        }
    } else {
        return None;
    };
    Some((v * 1_000_000_000).div_euclid(per_sec))
}

bif!("erlang:fun_info/2", fn fun_info2(f, item) {
    if !is_fun(f) || !is_atom(item) { return badarg(); }
    let ext = crate::funs::is_external(f);
    let v = match item {
        ARITY => small(crate::funs::fun_arity(f) as i64),
        MODULE => crate::funs::fun_module(f),
        NAME => if ext { crate::funs::fun_name(f) } else { atoms::intern("-anonymous-") },
        ENV => build::list(&crate::funs::fun_free(f)),
        TYPE => if ext { EXTERNAL } else { LOCAL },
        _ => return badarg(),
    };
    build::tuple(&[item, v])
});
bif!("erlang:module_loaded/1", fn module_loaded(_m) { TRUE });
bif!("erlang:make_fun/3", fn make_fun3(m, f, a) {
    if !is_atom(m) || !is_atom(f) || !is_small(a) || small_val(a) < 0 || small_val(a) > 255 { return badarg(); }
    crate::funs::export_fun(m, f, small_val(a) as u32).unwrap_or_else(|| badarg())
});
bif!("erlang:function_exported/3", fn function_exported(m, f, a) {
    if !is_small(a) { return badarg(); }
    boolean(crate::funs::rt_export_lookup(m, f, small_val(a) as u32) >= 0)
});

// host_io overrides: the request comes in as the "stdin" binary, the framed
// response goes out as "stdout".
bif!("host_io:read_stdin/0", fn read_stdin() { proc().input });
unsafe fn append_output(b: Term) {
    let p = proc();
    if !is_binary_obj(p.output) {
        p.output = b;
        return;
    }
    let mut w = crate::bits::Writer::new();
    w.push_bin(&bin_of(p.output), bin_of(p.output).bits);
    w.push_bin(&bin_of(b), bin_of(b).bits);
    p.output = w.finish();
}
bif!("host_io:write_stdout/1", fn write_stdout(b) {
    if !is_binary(b) { return badarg(); }
    append_output(b);
    OK
});
// io:put_chars to standard_io appends to stdout like erl does; standard_error is dropped.
bif!("io:put_chars/1", fn io_put_chars1(cs) { io_put_chars2(atoms::intern("standard_io"), cs) });
bif!("io:put_chars/2", fn io_put_chars2(dev, cs) {
    let b = crate::binmod::c2b(cs, UNICODE);
    if !is_binary(b) { return badarg(); }
    if dev != atoms::intern("standard_error") { append_output(b); }
    OK
});

bif!("crypto:strong_rand_bytes/1", fn strong_rand_bytes(n) {
    if !is_small(n) || small_val(n) < 0 { return badarg(); }
    let mut v = vec![0u8; small_val(n) as usize];
    crate::host_random(&mut v);
    build::binary(&v)
});

static mut DEBUG: bool = false;
pub fn debug_enabled() -> bool {
    unsafe { DEBUG }
}
/// Turns on runtime debug logging (undef calls, ...).
#[no_mangle]
pub unsafe extern "C" fn rt_set_debug(on: i32) {
    DEBUG = on != 0;
}

bif!("erlang:md5/1", fn md5(data) {
    use md5::{Digest, Md5};
    let mut w = crate::bits::Writer::new();
    if !crate::bits::iolist(data, &mut w, false) { return badarg(); }
    let d = Md5::digest(&w.buf);
    build::binary(&d)
});

// math: floats in, floats out; a non-finite result is badarith, like ERTS.
unsafe fn math1(a: Term, f: fn(f64) -> f64) -> Term {
    let Some(x) = num::to_f64(a) else { return badarg() };
    let r = f(x);
    if r.is_finite() { build::float(r) } else { error(BADARITH) }
}
unsafe fn math2(a: Term, b: Term, f: fn(f64, f64) -> f64) -> Term {
    let (Some(x), Some(y)) = (num::to_f64(a), num::to_f64(b)) else { return badarg() };
    let r = f(x, y);
    if r.is_finite() { build::float(r) } else { error(BADARITH) }
}
bif!("math:floor/1", fn m_floor(a) { math1(a, f64::floor) });
bif!("math:ceil/1", fn m_ceil(a) { math1(a, f64::ceil) });
bif!("math:sqrt/1", fn m_sqrt(a) { math1(a, f64::sqrt) });
bif!("math:exp/1", fn m_exp(a) { math1(a, f64::exp) });
bif!("math:log/1", fn m_log(a) { math1(a, f64::ln) });
bif!("math:log2/1", fn m_log2(a) { math1(a, f64::log2) });
bif!("math:log10/1", fn m_log10(a) { math1(a, f64::log10) });
bif!("math:sin/1", fn m_sin(a) { math1(a, f64::sin) });
bif!("math:cos/1", fn m_cos(a) { math1(a, f64::cos) });
bif!("math:tan/1", fn m_tan(a) { math1(a, f64::tan) });
bif!("math:asin/1", fn m_asin(a) { math1(a, f64::asin) });
bif!("math:acos/1", fn m_acos(a) { math1(a, f64::acos) });
bif!("math:atan/1", fn m_atan(a) { math1(a, f64::atan) });
bif!("math:sinh/1", fn m_sinh(a) { math1(a, f64::sinh) });
bif!("math:cosh/1", fn m_cosh(a) { math1(a, f64::cosh) });
bif!("math:tanh/1", fn m_tanh(a) { math1(a, f64::tanh) });
bif!("math:asinh/1", fn m_asinh(a) { math1(a, f64::asinh) });
bif!("math:acosh/1", fn m_acosh(a) { math1(a, f64::acosh) });
bif!("math:atanh/1", fn m_atanh(a) { math1(a, f64::atanh) });
bif!("math:pow/2", fn m_pow(a, b) { math2(a, b, f64::powf) });
bif!("math:atan2/2", fn m_atan2(a, b) { math2(a, b, f64::atan2) });
bif!("math:fmod/2", fn m_fmod(a, b) { math2(a, b, |x, y| x % y) });
bif!("math:erf/1", fn m_erf(a) { math1(a, erf) });
bif!("math:erfc/1", fn m_erfc(a) { math1(a, |x| 1.0 - erf(x)) });
fn erf(x: f64) -> f64 {
    // Abramowitz-Stegun 7.1.26 is too coarse; use a series/continued fraction split.
    let t = 1.0 / (1.0 + 0.5 * x.abs());
    let y = 1.0 - t * (-x * x - 1.26551223 + t * (1.00002368 + t * (0.37409196 + t * (0.09678418
        + t * (-0.18628806 + t * (0.27886807 + t * (-1.13520398 + t * (1.48851587
        + t * (-0.82215223 + t * 0.17087277))))))))).exp();
    if x >= 0.0 { y } else { -y }
}

// ---------------------------------------------------------------------------
// Process dictionary keys, term-to-text, io, time of day

bif!("erlang:get_keys/0", fn get_keys0() {
    let v: Vec<Term> = build::list_to_vec(proc().pdict).unwrap_or_default().iter().map(|&kv| tuple_get(kv, 0)).collect();
    build::list(&v)
});
bif!("erlang:get_keys/1", fn get_keys1(val) {
    let v: Vec<Term> = build::list_to_vec(proc().pdict)
        .unwrap_or_default()
        .iter()
        .filter(|&&kv| cmp::exact_eq(tuple_get(kv, 1), val))
        .map(|&kv| tuple_get(kv, 0))
        .collect();
    build::list(&v)
});
bif!("erlang:pid_to_list/1", fn pid_to_list(p) {
    if !is_pid(p) { return badarg(); }
    str_list(&crate::fmt::show(p))
});
bif!("erlang:ref_to_list/1", fn ref_to_list(r) { str_list(&crate::fmt::show(r)) });
bif!("erlang:fun_to_list/1", fn fun_to_list(f) { str_list(&crate::fmt::show(f)) });
// ERTS's default printable range (erl +pc latin1)
bif!("io:printable_range/0", fn printable_range() { atoms::intern("latin1") });

/// string:list_to_float/1 (the NIF behind string:to_float/1): the longest prefix that
/// is an Erlang float, as {Float, Rest}, or {error, no_float}.
bif!("string:list_to_float/1", fn string_list_to_float(l) {
    let mut chars = String::new();
    let mut x = l;
    while is_list(x) && chars.len() < 400 {
        let h = head(x);
        if !is_small(h) || !(0..128).contains(&small_val(h)) { break; }
        chars.push(small_val(h) as u8 as char);
        x = tail(x);
    }
    // try prefixes from longest to shortest
    for n in (3..=chars.len()).rev() {
        if let Some(f) = parse_erlang_float(&chars[..n]) {
            let mut rest = l;
            for _ in 0..n { rest = tail(rest); }
            return build::tuple(&[build::float(f), rest]);
        }
    }
    build::tuple(&[ERROR, atoms::intern("no_float")])
});

/// {{Y, M, D}, {H, Mi, S}} in UTC for seconds since the epoch. Workers run on UTC, so
/// local time is universal time.
unsafe fn datetime(secs: i64) -> Term {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // days to civil date (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    build::tuple(&[
        build::tuple(&[small(y), small(m), small(d)]),
        build::tuple(&[small(rem / 3600), small(rem / 60 % 60), small(rem % 60)]),
    ])
}
bif!("erlang:universaltime/0", fn universaltime() { datetime((now_ns() / 1_000_000_000) as i64) });
bif!("erlang:localtime/0", fn localtime() { universaltime() });
bif!("erlang:universaltime_to_localtime/1", fn ut_to_lt(dt) { dt });
bif!("erlang:localtime_to_universaltime/1", fn lt_to_ut(dt) { dt });
bif!("erlang:localtime_to_universaltime/2", fn lt_to_ut2(dt, _dst) { dt });
bif!("erlang:posixtime_to_universaltime/1", fn posix_to_ut(s) {
    match crate::num::to_i64(s) { Some(v) => datetime(v), None => badarg() }
});
bif!("erlang:date/0", fn date0() { tuple_get(universaltime(), 0) });
bif!("erlang:time/0", fn time0() { tuple_get(universaltime(), 1) });

// init: a Worker has no command line, so no emulator flags or plain arguments.
// (erl_features asks for 'enable-feature' flags the first time an atom is printed.)
bif!("init:get_arguments/0", fn init_get_arguments() { NIL });
bif!("init:get_argument/1", fn init_get_argument(_f) { ERROR });
bif!("init:get_plain_arguments/0", fn init_get_plain_arguments() { NIL });
