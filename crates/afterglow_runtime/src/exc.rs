//! Exceptions. A raising function stores class, reason and a raw stacktrace in the
//! process and returns NON_VALUE; generated code propagates it to the nearest
//! handler. The raw stacktrace is {Class, Frames}.

use crate::atoms::*;
use crate::build;
use crate::roots::proc;
use crate::term::*;

pub unsafe fn raise(class: Term, reason: Term) -> Term {
    raise_with(class, reason, NIL)
}

pub unsafe fn raise_with(class: Term, reason: Term, frames: Term) -> Term {
    if crate::bifs::debug_enabled() {
        crate::host_log(&format!("raise {}:{}", crate::fmt::show(class), crate::fmt::show(reason)));
    }
    let p = proc();
    p.exc_class = class;
    p.exc_reason = reason;
    p.exc_trace = NIL;
    let t = build::tuple(&[class, frames]);
    p.exc_trace = t;
    NON_VALUE
}

pub unsafe fn error(reason: Term) -> Term {
    raise(ERROR, reason)
}

pub unsafe fn error2(tag: Term, v: Term) -> Term {
    let r = build::tuple(&[tag, v]);
    raise(ERROR, r)
}

pub unsafe fn badarg() -> Term {
    error(BADARG)
}

#[no_mangle]
pub unsafe extern "C" fn rt_raise(class: Term, reason: Term) -> Term {
    raise(class, reason)
}
#[no_mangle]
pub unsafe extern "C" fn rt_error(reason: Term) -> Term {
    error(reason)
}
#[no_mangle]
pub unsafe extern "C" fn rt_error2(tag: Term, v: Term) -> Term {
    error2(tag, v)
}
/// function_clause, recording the function in the stacktrace.
#[no_mangle]
pub unsafe extern "C" fn rt_function_clause(m: Term, f: Term, a: u32) -> Term {
    let frame = build::tuple(&[m, f, small(a as i64), NIL]);
    let frames = build::list(&[frame]);
    raise_with(ERROR, FUNCTION_CLAUSE, frames)
}
#[no_mangle]
pub unsafe extern "C" fn rt_undef(m: Term, f: Term, a: u32) -> Term {
    if crate::bifs::debug_enabled() {
        crate::host_log(&format!("undef {}:{}/{}", crate::fmt::show(m), crate::fmt::show(f), a));
    }
    let frame = build::tuple(&[m, f, small(a as i64), NIL]);
    let frames = build::list(&[frame]);
    raise_with(ERROR, UNDEF, frames)
}
/// Opcodes this runtime does not implement (receive in tier 1).
#[no_mangle]
pub unsafe extern "C" fn rt_unsupported(op: Term) -> Term {
    error2(NOTSUP, op)
}

#[no_mangle]
pub unsafe extern "C" fn rt_exc_class() -> Term {
    proc().exc_class
}
#[no_mangle]
pub unsafe extern "C" fn rt_exc_reason() -> Term {
    proc().exc_reason
}
#[no_mangle]
pub unsafe extern "C" fn rt_exc_trace() -> Term {
    proc().exc_trace
}

/// The value of `catch Expr` after an exception.
#[no_mangle]
pub unsafe extern "C" fn rt_catch_value() -> Term {
    let p = proc();
    if p.exc_class == THROW {
        p.exc_reason
    } else if p.exc_class == ERROR {
        let st = rt_build_stacktrace(p.exc_trace);
        let inner = build::tuple(&[p.exc_reason, st]);
        build::tuple(&[EXIT_UPPER, inner])
    } else {
        build::tuple(&[EXIT_UPPER, p.exc_reason])
    }
}

/// `raise Trace Value`: rethrow with the class kept in the raw trace.
#[no_mangle]
pub unsafe extern "C" fn rt_raise_raw(trace: Term, value: Term) -> Term {
    if is_tuple(trace) && tuple_arity(trace) == 2 {
        let class = tuple_get(trace, 0);
        let frames = tuple_get(trace, 1);
        return raise_with(class, value, frames);
    }
    error(value)
}

#[no_mangle]
pub unsafe extern "C" fn rt_build_stacktrace(raw: Term) -> Term {
    if is_tuple(raw) && tuple_arity(raw) == 2 {
        tuple_get(raw, 1)
    } else {
        NIL
    }
}

/// erlang:raise/3
pub unsafe fn raise3(class: Term, reason: Term, trace: Term) -> Term {
    if class != ERROR && class != EXIT && class != THROW {
        return badarg();
    }
    raise_with(class, reason, trace)
}

/// Describes the current exception for logs: class:reason.
pub unsafe fn describe() -> String {
    let p = proc();
    format!("{}:{}", crate::fmt::show(p.exc_class), crate::fmt::show(p.exc_reason))
}

/// Called by generated code when an exception leaves a function without a handler:
/// appends {M, F, A, []} to the raw stacktrace (at most 8 frames, like ERTS).
#[no_mangle]
pub unsafe extern "C" fn rt_frame(m: Term, f: Term, a: u32) {
    let p = proc();
    let raw = p.exc_trace;
    if !is_tuple(raw) || tuple_arity(raw) != 2 {
        return;
    }
    let frames = tuple_get(raw, 1);
    let Some(mut v) = build::list_to_vec(frames) else { return };
    if v.len() >= 8 {
        return;
    }
    if let Some(&last) = v.last() {
        // the raising BIF/function already added itself
        if is_tuple(last) && tuple_get(last, 0) == m && tuple_get(last, 1) == f && v.len() == 1 && tuple_get(last, 2) == small(a as i64) {
            return;
        }
    }
    v.push(build::tuple(&[m, f, small(a as i64), NIL]));
    let nf = build::list(&v);
    p.exc_trace = build::tuple(&[tuple_get(raw, 0), nf]);
}
