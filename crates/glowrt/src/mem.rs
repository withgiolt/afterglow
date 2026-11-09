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

pub const STACK_WORDS: usize = 16 * 1024; // 128 KiB; overflow raises system_limit
pub static mut STACK_BASE: u32 = 0;
pub static mut STACK_TOP: u32 = 0;

const MIN_BLOCK_WORDS: usize = 8 * 1024; // 64 KiB

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
//
// Cheney over a chain of to-space blocks: the first is sized from the previous live
// size, more are added when it fills. Binaries' bytes live in the arena, outside the
// heap; when the arena has grown enough, live byte ranges are compacted into a fresh
// arena during the same collection.

static mut LAST_LIVE: usize = 0;
/// Arena bytes allocated since the last collection; past ARENA_GC_BYTES a GC is asked
/// for and the arena compacted.
pub static mut ARENA_SINCE_GC: usize = 0;
const ARENA_GC_BYTES: usize = 8 << 20;

struct ToBlock {
    start: *mut u64,
    end: *mut u64,
    hp: *mut u64,
}

struct Gc {
    from: Vec<(usize, usize)>, // sorted
    to: Vec<ToBlock>,
    bins: Vec<*mut u64>, // copied binary objects
}

impl Gc {
    #[inline]
    fn in_from(&self, addr: usize) -> bool {
        // binary search over sorted, non-overlapping ranges
        let i = self.from.partition_point(|&(s, _)| s <= addr);
        i > 0 && addr < self.from[i - 1].1
    }

    #[inline]
    unsafe fn alloc(&mut self, n: usize) -> *mut u64 {
        let b = self.to.last_mut().unwrap();
        if (b.end as usize - b.hp as usize) / 8 >= n {
            let r = b.hp;
            b.hp = r.add(n);
            return r;
        }
        let words = n.max(MIN_BLOCK_WORDS);
        let p = new_block(words);
        self.to.push(ToBlock { start: p, end: p.add(words), hp: p.add(n) });
        p
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
                let n = self.alloc(2);
                *n = *p;
                *n.add(1) = *p.add(1);
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
                let n = self.alloc(size);
                core::ptr::copy_nonoverlapping(p, n, size);
                if hdr_sub(h) == SUB_BIN {
                    self.bins.push(n);
                }
                let nt = make_boxed(n);
                *p = nt;
                nt
            }
            _ => t,
        }
    }

    unsafe fn scan(&mut self) {
        let mut bi = 0;
        while bi < self.to.len() {
            let mut s = self.to[bi].start;
            while s < self.to[bi].hp {
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
            bi += 1;
        }
    }

    /// Moves the live byte ranges of arena binaries into a fresh arena.
    unsafe fn compact_arena(&mut self) {
        let arena = &mut *core::ptr::addr_of_mut!(ARENA);
        // (start, end, object) for every binary whose bytes are in the arena
        let mut spans: Vec<(usize, usize, *mut u64)> = Vec::new();
        for &o in &self.bins {
            let bits = small_val(*o.add(1)) as u64;
            let base = small_val(*o.add(2)) as usize;
            let bitoff = small_val(*o.add(3)) as u64;
            if arena.contains(base) {
                let len = ((bitoff + bits + 7) / 8) as usize;
                spans.push((base, base + len, o));
            }
        }
        spans.sort_by_key(|s| s.0);
        let mut fresh = Arena::new(false);
        let mut i = 0;
        while i < spans.len() {
            let (rs, mut re) = (spans[i].0, spans[i].1);
            let mut j = i + 1;
            while j < spans.len() && spans[j].0 <= re {
                re = re.max(spans[j].1);
                j += 1;
            }
            let np = fresh.alloc((re - rs).max(1));
            core::ptr::copy_nonoverlapping(rs as *const u8, np, re - rs);
            for k in i..j {
                let o = spans[k].2;
                *o.add(2) = small((np as usize + (spans[k].0 - rs)) as i64);
            }
            i = j;
        }
        arena.free_all();
        *arena = fresh;
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
    let mut from: Vec<(usize, usize)> =
        blocks.iter().map(|b| (b.start as usize, b.start.add(b.words) as usize)).collect();
    from.sort();
    let old: Vec<Block> = core::mem::take(blocks);
    let first = (LAST_LIVE * 3).max(MIN_BLOCK_WORDS);
    let to = new_block(first);
    let mut gc = Gc { from, to: vec![ToBlock { start: to, end: to.add(first), hp: to }], bins: Vec::new() };

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
    let compact = ARENA_SINCE_GC >= ARENA_GC_BYTES;
    if compact {
        gc.compact_arena();
    }
    ARENA_SINCE_GC = 0;

    for b in old {
        // Zero before freeing so a later request can't read this one's data.
        core::ptr::write_bytes(b.start, 0, b.words);
        dealloc(b.start as *mut u8, block_layout(b.words));
    }
    let live_words: usize = gc.to.iter().map(|b| (b.hp as usize - b.start as usize) / 8).sum();
    LAST_LIVE = live_words;
    let last = gc.to.last().unwrap();
    STATE.hp = last.hp as usize as u32;
    STATE.hlim = last.end as usize as u32;
    STATE.gc_pending = 0;
    STATS_GCS += 1;
    if crate::bifs::debug_enabled() {
        crate::host_log(&format!(
            "gc #{}: used {} words, live {} words in {} blocks, live x {}, stack {} words, arena {} KiB{}",
            STATS_GCS,
            used,
            live_words,
            gc.to.len(),
            live,
            (STACK_TOP - sp) / 8,
            (*core::ptr::addr_of!(ARENA)).bytes_reserved() / 1024,
            if compact { " (compacted)" } else { "" }
        ));
    }
}

/// Frees every heap block, keeping (or making) one of the minimum size, zeroed.
pub unsafe fn reset_heap() {
    let blocks = &mut *core::ptr::addr_of_mut!(BLOCKS);
    let hp = STATE.hp as usize;
    let mut keep: Option<Block> = None;
    for b in blocks.drain(..) {
        let end = b.start.add(b.words) as usize;
        let used_end = if hp > b.start as usize && hp <= end { hp } else { end };
        if keep.is_none() && b.words == MIN_BLOCK_WORDS {
            core::ptr::write_bytes(b.start as *mut u8, 0, used_end - b.start as usize);
            keep = Some(b);
        } else {
            core::ptr::write_bytes(b.start, 0, b.words);
            dealloc(b.start as *mut u8, block_layout(b.words));
        }
    }
    let keep = match keep {
        Some(b) => {
            blocks.push(b);
            blocks.pop().unwrap()
        }
        None => {
            let p = new_block(MIN_BLOCK_WORDS);
            blocks.pop().unwrap();
            Block { start: p, words: MIN_BLOCK_WORDS }
        }
    };
    STATE.hp = keep.start as usize as u32;
    STATE.hlim = keep.start.add(keep.words) as usize as u32;
    STATE.gc_pending = 0;
    LAST_LIVE = 0;
    ARENA_SINCE_GC = 0;
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
    /// Exact end address of the most recent allocation (for in-place appends).
    exact_end: usize,
}

const CHUNK: usize = 16 * 1024;

impl Arena {
    pub const fn new(permanent: bool) -> Self {
        Arena { chunks: Vec::new(), pos: 0, permanent, exact_end: 0 }
    }

    pub unsafe fn alloc(&mut self, len: usize) -> *mut u8 {
        let n = (len + 7) & !7;
        if let Some(&(p, size)) = self.chunks.last() {
            if self.pos + n <= size {
                let r = p.add(self.pos);
                self.pos += n;
                self.exact_end = r as usize + len;
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
        self.exact_end = p as usize + len;
        p
    }

    /// Tries to grow (or shrink) the most recent allocation in place. Only succeeds
    /// when `start + old_len` is exactly where the last allocation ends, so a
    /// sub-binary can never grow over bytes that belong to its parent.
    pub unsafe fn try_extend(&mut self, start: *const u8, old_len: usize, new_len: usize) -> bool {
        if let Some(&(p, size)) = self.chunks.last() {
            let off = start as usize;
            if off >= p as usize
                && off + old_len == self.exact_end
                && (off - p as usize) + new_len <= size
            {
                self.pos = (off - p as usize) + ((new_len + 7) & !7);
                self.exact_end = off + new_len;
                return true;
            }
        }
        false
    }

    /// Zeroes and frees every chunk.
    pub unsafe fn free_all(&mut self) {
        for (i, &(p, sz)) in self.chunks.iter().enumerate() {
            let used = if i == self.chunks.len() - 1 { self.pos } else { sz };
            core::ptr::write_bytes(p, 0, used);
            dealloc(p, Layout::from_size_align(sz, 8).unwrap());
        }
        self.chunks.clear();
        self.pos = 0;
        self.exact_end = 0;
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
        self.exact_end = 0;
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
        ARENA_SINCE_GC += bytes;
        if ARENA_SINCE_GC >= ARENA_GC_BYTES {
            STATE.gc_pending = 1;
        }
        (*core::ptr::addr_of_mut!(ARENA)).alloc(bytes.max(1))
    }
}
