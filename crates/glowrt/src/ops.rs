//! Helpers called by generated code for instructions that are not BIFs: type tests,
//! float registers, fun calls and apply.

use crate::atoms::*;
use crate::build;
use crate::exc::{error, error2};
use crate::funs;
use crate::num;
use crate::term::*;

#[no_mangle]
pub extern "C" fn rt_is_integer(t: Term) -> i32 {
    is_integer(t) as i32
}
#[no_mangle]
pub extern "C" fn rt_is_float(t: Term) -> i32 {
    is_float(t) as i32
}
#[no_mangle]
pub extern "C" fn rt_is_number(t: Term) -> i32 {
    is_number(t) as i32
}
#[no_mangle]
pub extern "C" fn rt_is_pid(t: Term) -> i32 {
    is_pid(t) as i32
}
#[no_mangle]
pub extern "C" fn rt_is_port(t: Term) -> i32 {
    is_port(t) as i32
}
#[no_mangle]
pub extern "C" fn rt_is_reference(t: Term) -> i32 {
    is_ref(t) as i32
}
#[no_mangle]
pub extern "C" fn rt_is_list(t: Term) -> i32 {
    (t == NIL || is_list(t)) as i32
}
#[no_mangle]
pub extern "C" fn rt_is_tuple(t: Term) -> i32 {
    is_tuple(t) as i32
}
#[no_mangle]
pub extern "C" fn rt_is_binary(t: Term) -> i32 {
    is_binary(t) as i32
}
#[no_mangle]
pub extern "C" fn rt_is_bitstring(t: Term) -> i32 {
    is_bitstring(t) as i32
}
#[no_mangle]
pub extern "C" fn rt_is_map(t: Term) -> i32 {
    is_map(t) as i32
}
#[no_mangle]
pub extern "C" fn rt_is_boolean(t: Term) -> i32 {
    (t == atom(0) || t == atom(1)) as i32
}
#[no_mangle]
pub extern "C" fn rt_is_function(t: Term) -> i32 {
    is_fun(t) as i32
}
#[no_mangle]
pub unsafe extern "C" fn rt_is_function2(t: Term, n: Term) -> i32 {
    (is_fun(t) && is_small(n) && funs::fun_arity(t) as i64 == small_val(n)) as i32
}
#[no_mangle]
pub unsafe extern "C" fn rt_test_arity(t: Term, n: i32) -> i32 {
    (is_tuple(t) && tuple_arity(t) == n as usize) as i32
}
#[no_mangle]
pub unsafe extern "C" fn rt_is_tagged_tuple(t: Term, n: i32, tag: Term) -> i32 {
    (is_tuple(t) && tuple_arity(t) == n as usize && n > 0 && tuple_get(t, 0) == tag) as i32
}
/// Arity of a tuple, or -1.
#[no_mangle]
pub unsafe extern "C" fn rt_tuple_arity(t: Term) -> i32 {
    if is_tuple(t) {
        tuple_arity(t) as i32
    } else {
        -1
    }
}

#[no_mangle]
pub unsafe extern "C" fn rt_box_float(f: f64) -> Term {
    build::float(f)
}
#[no_mangle]
pub unsafe extern "C" fn rt_float_val(t: Term) -> f64 {
    float_val(t)
}
#[no_mangle]
pub unsafe extern "C" fn rt_num_to_f64(t: Term) -> f64 {
    num::to_f64(t).unwrap_or(f64::NAN)
}

/// Checks a fun call: returns the table index of the fun's wrapper, or -1 with an
/// exception raised (badfun, badarity, or undef for an unknown external fun).
#[no_mangle]
pub unsafe extern "C" fn rt_fun_check(f: Term, arity: i32) -> i32 {
    if !is_fun(f) {
        error2(BADFUN, f);
        return -1;
    }
    if funs::fun_arity(f) != arity as u32 {
        // {badarity, {Fun, Args}}: args are in the generated function's locals, which
        // we can't see here; ERTS reports them, so the difftest compares only the class.
        let inner = build::tuple(&[f, NIL]);
        error2(BADARITY, inner);
        return -1;
    }
    let idx = small_val(*ptr(f).add(1)) as i32;
    if idx < 0 {
        crate::exc::rt_undef(funs::fun_module(f), funs::fun_name(f), arity as u32);
        return -1;
    }
    idx
}

/// apply/3 setup: stores the argument list in XREGS and returns an external fun for
/// M:F/length(Args), or NON_VALUE with an exception raised.
#[no_mangle]
pub unsafe extern "C" fn rt_apply_prepare(m: Term, f: Term, args: Term) -> Term {
    let Some(v) = build::list_to_vec(args) else { return error(BADARG) };
    if !is_atom(m) || !is_atom(f) {
        return error(BADARG);
    }
    let x = &mut *core::ptr::addr_of_mut!(crate::mem::XREGS);
    for (i, a) in v.iter().enumerate() {
        x[i] = *a;
    }
    funs::export_fun(m, f, v.len() as u32).unwrap()
}

/// apply/2 setup: stores the argument list in XREGS and returns the fun.
#[no_mangle]
pub unsafe extern "C" fn rt_apply_prepare_fun(fun: Term, args: Term) -> Term {
    let Some(v) = build::list_to_vec(args) else { return error(BADARG) };
    if !is_fun(fun) {
        return error2(BADFUN, fun);
    }
    let x = &mut *core::ptr::addr_of_mut!(crate::mem::XREGS);
    for (i, a) in v.iter().enumerate() {
        x[i] = *a;
    }
    fun
}

/// Length of a proper list (for apply), -1 otherwise.
#[no_mangle]
pub unsafe extern "C" fn rt_list_len(l: Term) -> i32 {
    match build::list_to_vec(l) {
        Some(v) => v.len() as i32,
        None => -1,
    }
}
