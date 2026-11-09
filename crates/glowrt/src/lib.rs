//! glowrt: the runtime linked into every glowc-compiled program.
//!
//! The generated module imports these functions and this module's memory. Runtime
//! BIFs are exported under their Erlang names ("erlang:length/1"); glowc uses the
//! export list as its override table: an MFA exported here is called instead of
//! compiling that function from its .beam.

pub mod atoms;
pub mod bifs;
pub mod build;
pub mod cmp;
pub mod etf;
pub mod exc;
pub mod fmt;
pub mod funs;
pub mod mem;
pub mod num;
pub mod ops;
pub mod bits;
pub mod maps;
pub mod binmod;
pub mod roots;
pub mod sched;
pub mod term;

use term::*;

static mut LITERALS: Vec<Term> = Vec::new();

/// Called once per instance. Blobs:
/// - atoms: names separated by NUL (fixed atoms first, then the compiler's)
/// - exports: u32 LE quadruples (module atom, function atom, arity, table index)
/// - literals: (u32 LE length, ETF bytes) repeated
/// Returns the address of the literal table (one term per literal).
#[no_mangle]
pub unsafe extern "C" fn rt_init(
    atoms_ptr: u32,
    atoms_len: u32,
    exports_ptr: u32,
    exports_len: u32,
    lits_ptr: u32,
    lits_len: u32,
) -> u32 {
    std::panic::set_hook(Box::new(|info| host_log(&format!("glowrt panic: {}", info))));
    mem::init();
    let slice = |p: u32, n: u32| core::slice::from_raw_parts(p as usize as *const u8, n as usize);
    atoms::load(slice(atoms_ptr, atoms_len));
    funs::load(slice(exports_ptr, exports_len));
    mem::TO_PERM = true;
    let lits = slice(lits_ptr, lits_len);
    let table = &mut *core::ptr::addr_of_mut!(LITERALS);
    let mut pos = 0;
    while pos < lits.len() {
        let n = u32::from_le_bytes([lits[pos], lits[pos + 1], lits[pos + 2], lits[pos + 3]]) as usize;
        pos += 4;
        let t = etf::binary_to_term(&lits[pos..pos + n], false)
            .unwrap_or_else(|| panic!("cannot decode literal {}", table.len()));
        table.push(t);
        pos += n;
    }
    mem::TO_PERM = false;
    table.as_ptr() as usize as u32
}

/// Scratch space for the generated module's init blobs, in the permanent area.
#[no_mangle]
pub unsafe extern "C" fn rt_static_alloc(len: u32) -> u32 {
    (*core::ptr::addr_of_mut!(mem::PERM)).alloc(len as usize) as usize as u32
}

#[no_mangle]
pub unsafe extern "C" fn rt_state() -> u32 {
    core::ptr::addr_of!(mem::STATE) as usize as u32
}
#[no_mangle]
pub unsafe extern "C" fn rt_xregs() -> u32 {
    core::ptr::addr_of!(mem::XREGS) as usize as u32
}
#[no_mangle]
pub unsafe extern "C" fn rt_stack_top() -> u32 {
    mem::STACK_TOP
}
#[no_mangle]
pub unsafe extern "C" fn rt_stack_base() -> u32 {
    mem::STACK_BASE
}

/// Starts a request: wipes the previous one (heap, arena, process state) and returns
/// a buffer of `len` bytes for the host to write the request into.
#[no_mangle]
pub unsafe extern "C" fn rt_begin(len: u32) -> u32 {
    mem::reset_heap();
    (*core::ptr::addr_of_mut!(mem::ARENA)).reset();
    roots::reset();
    bifs::reset();
    sched::reset();
    let buf = mem::balloc(len as usize);
    roots::proc().input = build::bin_ref(buf, 0, len as u64 * 8);
    buf as usize as u32
}

/// Output written by host_io:write_stdout/1, packed as ptr << 32 | len.
#[no_mangle]
pub unsafe extern "C" fn rt_output() -> u64 {
    let o = roots::proc().output;
    if !is_binary_obj(o) {
        return 0;
    }
    pack_bytes(&bin_of(o).to_vec())
}

unsafe fn pack_bytes(bytes: &[u8]) -> u64 {
    let p = mem::balloc(bytes.len());
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
    ((p as usize as u64) << 32) | bytes.len() as u64
}

/// term_to_binary of a result, for the difftest runner. 0 if it can't be encoded.
#[no_mangle]
pub unsafe extern "C" fn rt_result_etf(t: Term) -> u64 {
    match etf::term_to_binary(t) {
        Some(v) => pack_bytes(&v),
        None => 0,
    }
}

/// The current exception as text, packed as ptr << 32 | len.
#[no_mangle]
pub unsafe extern "C" fn rt_exception_text() -> u64 {
    let p = roots::proc();
    let st = exc::rt_build_stacktrace(p.exc_trace);
    let s = format!("{}, stacktrace: {}", exc::describe(), fmt::show(st));
    pack_bytes(s.as_bytes())
}

#[no_mangle]
pub unsafe extern "C" fn rt_stats_gcs() -> u64 {
    mem::gc_count()
}
#[no_mangle]
pub unsafe extern "C" fn rt_stats_heap_words() -> u64 {
    mem::heap_words_in_use() as u64
}
#[no_mangle]
pub unsafe extern "C" fn rt_stats_arena_bytes() -> u64 {
    (*core::ptr::addr_of!(mem::ARENA)).bytes_reserved() as u64
}

/// Debug aid: prints a term through the host.
#[no_mangle]
pub unsafe extern "C" fn rt_debug(t: Term) -> Term {
    host_log(&fmt::show(t));
    t
}

#[link(wasm_import_module = "host")]
extern "C" {
    fn log(ptr: u32, len: u32);
    fn random_bytes(ptr: u32, len: u32);
    fn now_us() -> f64;
}

/// Wall clock in microseconds (Date.now() in a Worker: frozen during compute).
pub fn host_now_us() -> f64 {
    unsafe { now_us() }
}

pub fn host_log(s: &str) {
    unsafe { log(s.as_ptr() as usize as u32, s.len() as u32) }
}

pub fn host_random(buf: &mut [u8]) {
    unsafe { random_bytes(buf.as_mut_ptr() as usize as u32, buf.len() as u32) }
}
