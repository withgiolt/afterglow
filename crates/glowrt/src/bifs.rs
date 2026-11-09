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

bif!("erlang:self/0", fn self_() { pid(0) });
bif!("erlang:node/0", fn node0() { NONODE });
bif!("erlang:node/1", fn node1(_x) { NONODE });
bif!("erlang:group_leader/0", fn group_leader() { pid(1) });
bif!("erlang:whereis/1", fn whereis(_n) { UNDEFINED });
bif!("erlang:monitor/2", fn monitor(_t, _p) { make_ref() });
bif!("erlang:demonitor/2", fn demonitor(_r, _o) { TRUE });
bif!("erlang:demonitor/1", fn demonitor1(_r) { TRUE });
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
pub fn float_short(f: f64) -> String {
    let s = format!("{:?}", f);
    // Rust prints 1e20 as "1e20" and 0.1 as "0.1"; Erlang short gives "1.0e20".
    if let Some(pos) = s.find('e') {
        let (m, e) = s.split_at(pos);
        let m = if m.contains('.') { m.to_string() } else { format!("{}.0", m) };
        format!("{}{}", m, e)
    } else {
        s
    }
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
bif!("os:getenv/1", fn getenv(_n) { FALSE });
bif!("os:system_time/1", fn os_system_time(_u) { small(0) });
bif!("erlang:system_time/1", fn system_time1(_u) { small(0) });
bif!("erlang:monotonic_time/0", fn monotonic0() { small(0) });
bif!("erlang:monotonic_time/1", fn monotonic1(_u) { small(0) });
bif!("erlang:module_loaded/1", fn module_loaded(_m) { TRUE });
bif!("erlang:function_exported/3", fn function_exported(m, f, a) {
    if !is_small(a) { return badarg(); }
    boolean(crate::funs::rt_export_lookup(m, f, small_val(a) as u32) >= 0)
});

// host_io overrides: the request comes in as the "stdin" binary, the framed
// response goes out as "stdout".
bif!("host_io:read_stdin/0", fn read_stdin() { proc().input });
bif!("host_io:write_stdout/1", fn write_stdout(b) {
    if !is_binary(b) { return badarg(); }
    proc().output = b;
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
