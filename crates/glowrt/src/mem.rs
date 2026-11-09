//! Memory: the process heap (bump allocation in blocks, copying GC), the per-request
//! binary arena, the permanent literal area, the Y stack and the X spill area.
//!
//! Allocation never collects. When a block is full a new one is chained and
//! `gc_pending` is set; generated code collects at its next safe point (function
//! entry or test_heap), where the live X registers are spilled and the Y stack is
//! exact.

use crate::term::*;
use std::alloc::{alloc_zeroed, dealloc, Layout};

#[repr(C)]
pub struct State {
    pub gc_pending: u32, // offset 0, read by generated code
    pub hp: u32,         // offset 4
    pub hlim: u32,       // offset 8
    pub _pad: u32,
}

pub static mut STATE: State = State { gc_pending: 0, hp: 0, hlim: 0, _pad: 0 };

pub const X_REGS: usize = 1024;
pub static mut XREGS: [u64; X_REGS] = [0; X_REGS];

pub const STACK_WORDS: usize = 256 * 1024;
pub static mut STACK_BASE: u32 = 0;
pub static mut STACK_TOP: u32 = 0;

const MIN_BLOCK_WORDS: usize = 32 * 1024; // 256 KiB

struct Block {
    start: *mut u64,
    words: usize,
}

static mut BLOCKS: Vec<Block> = Vec::new();
/// Words allocated in blocks other than the current one since the last collection.
static mut STATS_GCS: u64 = 0;

fn block_layout(words: usize) -> Layout {
    Layout::from_size_align(words * 8, 8).unwrap()
}

unsafe fn new_block(words: usize) -> *mut u64 {
    let p = alloc_zeroed(block_layout(words)) as *mut u64;
    if p.is_null() {
        panic!("out of memory allocating a heap block of {} words", words);
    }
    (*core::ptr::addr_of_mut!(BLOCKS)).push(Block { start: p, words });
    p
}

pub unsafe fn init() {
    let stack = alloc_zeroed(block_layout(STACK_WORDS)) as *mut u64;
    STACK_BASE = stack as usize as u32;
    STACK_TOP = stack.add(STACK_WORDS) as usize as u32;
    let p = new_block(MIN_BLOCK_WORDS);
    STATE.hp = p as usize as u32;
    STATE.hlim = p.add(MIN_BLOCK_WORDS) as usize as u32;
}

/// Allocates `words` on the process heap. Never collects.
#[inline]
pub unsafe fn alloc(words: usize) -> *mut u64 {
    let hp = STATE.hp as usize;
    let np = hp + words * 8;
    if np <= STATE.hlim as usize {
        STATE.hp = np as u32;
        return hp as *mut u64;
    }
    alloc_slow(words)
}

pub unsafe fn alloc_slow(words: usize) -> *mut u64 {
    let size = words.max(MIN_BLOCK_WORDS);
    let p = new_block(size);
    STATE.hp = p.add(words) as usize as u32;
    STATE.hlim = p.add(size) as usize as u32;
    STATE.gc_pending = 1;
    p
}

#[no_mangle]
pub unsafe extern "C" fn rt_alloc_slow(words: u32) -> u32 {
    alloc_slow(words as usize) as usize as u32
}

// ---------------------------------------------------------------------------
// Copying collector

struct Gc {
    from: Vec<(usize, usize)>,
    to_start: *mut u64,
    to_hp: *mut u64,
}

impl Gc {
    #[inline]
    fn in_from(&self, addr: usize) -> bool {
        self.from.iter().any(|&(s, e)| addr >= s && addr < e)
    }

    unsafe fn copy(&mut self, t: Term) -> Term {
        match t & TAG_MASK {
            TAG_LIST => {
                let p = ptr(t);
                if !self.in_from(p as usize) {
                    return t;
                }
                if *p == NON_VALUE {
                    return *p.add(1); // forwarded
                }
                let n = self.to_hp;
                *n = *p;
                *n.add(1) = *p.add(1);
                self.to_hp = n.add(2);
                let nt = make_list(n);
                *p = NON_VALUE;
                *p.add(1) = nt;
                nt
            }
            TAG_BOXED => {
                let p = ptr(t);
                if !self.in_from(p as usize) {
                    return t;
                }
                let h = *p;
                if h & TAG_MASK == TAG_BOXED {
                    return h; // forwarded
                }
                let size = 1 + hdr_arity(h) as usize;
                let n = self.to_hp;
                core::ptr::copy_nonoverlapping(p, n, size);
                self.to_hp = n.add(size);
                let nt = make_boxed(n);
                *p = nt;
                nt
            }
            _ => t,
        }
    }

    unsafe fn scan(&mut self) {
        let mut s = self.to_start;
        while s < self.to_hp {
            let w = *s;
            if w & TAG_MASK == 0 {
                let arity = hdr_arity(w) as usize;
                match hdr_sub(w) {
                    SUB_FLOAT | SUB_POS_BIG | SUB_NEG_BIG | SUB_REF => {}
                    _ => {
                        for i in 1..=arity {
                            *s.add(i) = self.copy(*s.add(i));
                        }
                    }
                }
                s = s.add(1 + arity);
            } else {
                *s = self.copy(*s);
                *s.add(1) = self.copy(*s.add(1));
                s = s.add(2);
            }
        }
    }
}

/// Collects with roots: x0..x(live-1) in XREGS, the Y stack from `sp` to the top, and
/// the runtime's own roots.
#[no_mangle]
pub unsafe extern "C" fn rt_gc(live: u32, sp: u32) {
    let blocks = &mut *core::ptr::addr_of_mut!(BLOCKS);
    let used: usize = blocks
        .iter()
        .map(|b| {
            let end = b.start.add(b.words) as usize;
            let top = if (STATE.hp as usize) >= b.start as usize && (STATE.hp as usize) <= end {
                STATE.hp as usize
            } else {
                end
            };
            (top - b.start as usize) / 8
        })
        .sum();
    let from: Vec<(usize, usize)> =
        blocks.iter().map(|b| (b.start as usize, b.start.add(b.words) as usize)).collect();
    let old: Vec<Block> = core::mem::take(blocks);
    let to_words = (used * 2).max(MIN_BLOCK_WORDS);
    let to = new_block(to_words);
    let mut gc = Gc { from, to_start: to, to_hp: to };

    let xregs = &mut *core::ptr::addr_of_mut!(XREGS);
    for i in 0..live as usize {
        xregs[i] = gc.copy(xregs[i]);
    }
    let mut y = sp as usize as *mut u64;
    let top = STACK_TOP as usize as *mut u64;
    while y < top {
        *y = gc.copy(*y);
        y = y.add(1);
    }
    crate::roots::for_each_root(|r| *r = gc.copy(*r));
    gc.scan();

    for b in old {
        // Zero before freeing so a later request can't read this one's data.
        core::ptr::write_bytes(b.start, 0, b.words);
        dealloc(b.start as *mut u8, block_layout(b.words));
    }
    STATE.hp = gc.to_hp as usize as u32;
    STATE.hlim = to.add(to_words) as usize as u32;
    STATE.gc_pending = 0;
    STATS_GCS += 1;
}

/// Frees every heap block but one, zeroes it and resets the bump pointer.
pub unsafe fn reset_heap() {
    let blocks = &mut *core::ptr::addr_of_mut!(BLOCKS);
    let keep = blocks.remove(0);
    for b in blocks.drain(..) {
        core::ptr::write_bytes(b.start, 0, b.words);
        dealloc(b.start as *mut u8, block_layout(b.words));
    }
    let used_end = if (STATE.hp as usize) > keep.start as usize
        && (STATE.hp as usize) <= keep.start.add(keep.words) as usize
    {
        STATE.hp as usize
    } else {
        keep.start.add(keep.words) as usize
    };
    core::ptr::write_bytes(keep.start as *mut u8, 0, used_end - keep.start as usize);
    STATE.hp = keep.start as usize as u32;
    STATE.hlim = keep.start.add(keep.words) as usize as u32;
    STATE.gc_pending = 0;
    blocks.push(keep);
}

pub fn gc_count() -> u64 {
    unsafe { STATS_GCS }
}

pub fn heap_words_in_use() -> usize {
    unsafe {
        let blocks = &*core::ptr::addr_of!(BLOCKS);
        blocks.iter().map(|b| b.words).sum()
    }
}

// ---------------------------------------------------------------------------
// Binary arena (per request) and permanent area (literals)

pub struct Arena {
    chunks: Vec<(*mut u8, usize)>,
    pos: usize,
    permanent: bool,
}

const CHUNK: usize = 64 * 1024;

impl Arena {
    pub const fn new(permanent: bool) -> Self {
        Arena { chunks: Vec::new(), pos: 0, permanent }
    }

    pub unsafe fn alloc(&mut self, n: usize) -> *mut u8 {
        let n = (n + 7) & !7;
        if let Some(&(p, size)) = self.chunks.last() {
            if self.pos + n <= size {
                let r = p.add(self.pos);
                self.pos += n;
                return r;
            }
        }
        let size = n.max(CHUNK);
        let p = alloc_zeroed(Layout::from_size_align(size, 8).unwrap());
        if p.is_null() {
            panic!("out of memory allocating {} bytes of binary data", n);
        }
        self.chunks.push((p, size));
        self.pos = n;
        p
    }

    /// Tries to grow the most recent allocation in place (used for binary appends).
    pub unsafe fn try_extend(&mut self, start: *const u8, old_len: usize, new_len: usize) -> bool {
        if let Some(&(p, size)) = self.chunks.last() {
            let old_end = ((old_len + 7) & !7) + (start as usize - p as usize);
            if start as usize >= p as usize
                && old_end == self.pos
                && (start as usize - p as usize) + new_len <= size
            {
                self.pos = (start as usize - p as usize) + ((new_len + 7) & !7);
                return true;
            }
        }
        false
    }

    pub fn contains(&self, addr: usize) -> bool {
        self.chunks.iter().any(|&(p, size)| addr >= p as usize && addr < p as usize + size)
    }

    pub unsafe fn reset(&mut self) {
        debug_assert!(!self.permanent);
        if self.chunks.is_empty() {
            return;
        }
        let (first, size) = self.chunks[0];
        for (i, &(p, sz)) in self.chunks.iter().enumerate() {
            let used = if i == self.chunks.len() - 1 { self.pos } else { sz };
            core::ptr::write_bytes(p, 0, used);
            if i > 0 {
                dealloc(p, Layout::from_size_align(sz, 8).unwrap());
            }
        }
        self.chunks.clear();
        self.chunks.push((first, size));
        self.pos = 0;
    }

    pub fn bytes_reserved(&self) -> usize {
        self.chunks.iter().map(|c| c.1).sum()
    }
}

pub static mut ARENA: Arena = Arena::new(false);
pub static mut PERM: Arena = Arena::new(true);

/// Where runtime-built terms go: the process heap, or the permanent area while
/// literals are decoded at init.
pub static mut TO_PERM: bool = false;

#[inline]
pub unsafe fn halloc(words: usize) -> *mut u64 {
    if TO_PERM {
        (*core::ptr::addr_of_mut!(PERM)).alloc(words * 8) as *mut u64
    } else {
        alloc(words)
    }
}

#[inline]
pub unsafe fn balloc(bytes: usize) -> *mut u8 {
    if TO_PERM {
        (*core::ptr::addr_of_mut!(PERM)).alloc(bytes.max(1))
    } else {
        (*core::ptr::addr_of_mut!(ARENA)).alloc(bytes.max(1))
    }
}
