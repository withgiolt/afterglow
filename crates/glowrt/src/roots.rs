//! Process state held by the runtime that the GC must treat as roots: the current
//! exception, the process dictionary, and the request input/output.

use crate::term::*;

pub struct Proc {
    pub exc_class: Term,
    pub exc_reason: Term,
    /// Raw stacktrace: {Class, List}, opaque to Erlang code.
    pub exc_trace: Term,
    pub pdict: Term, // list of {Key, Value}
    pub input: Term,
    pub output: Term,
    pub refs: u64,
}

pub static mut PROC: Proc = Proc {
    exc_class: NIL,
    exc_reason: NIL,
    exc_trace: NIL,
    pdict: NIL,
    input: NIL,
    output: NIL,
    refs: 0,
};

pub fn proc() -> &'static mut Proc {
    unsafe { &mut *core::ptr::addr_of_mut!(PROC) }
}

pub unsafe fn for_each_root(mut f: impl FnMut(&mut Term)) {
    let p = proc();
    f(&mut p.exc_class);
    f(&mut p.exc_reason);
    f(&mut p.exc_trace);
    f(&mut p.pdict);
    f(&mut p.input);
    f(&mut p.output);
    crate::bifs::for_each_bif_root(&mut f);
}

pub fn reset() {
    let p = proc();
    p.exc_class = NIL;
    p.exc_reason = NIL;
    p.exc_trace = NIL;
    p.pdict = NIL;
    p.input = NIL;
    p.output = NIL;
}
