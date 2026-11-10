//! Term representation: 64-bit tagged words, following ERTS's 64-bit tagging.
//!
//! Primary tag (2 bits): 00 header, 01 list (cons pointer), 10 boxed pointer, 11 immediate.
//! Immediate1 (4 bits): 0011 pid, 0111 port, 1011 immediate2, 1111 small integer (60 bits).
//! Immediate2 (6 bits): 001011 atom, 011011 catch, 111011 nil.
//! Pointers are 32-bit addresses into linear memory, 8-byte aligned.
//!
//! Boxed objects start with a header word: arity << 6 | subtag << 2. Every word after
//! the header is a valid term (raw values are stored as small integers), except in
//! floats, bignums and references, whose payload is raw.

pub type Term = u64;

pub const NON_VALUE: Term = 0;
pub const NIL: Term = 0x3B;

pub const TAG_MASK: u64 = 3;
pub const TAG_LIST: u64 = 1;
pub const TAG_BOXED: u64 = 2;
pub const TAG_IMMED: u64 = 3;

pub const SUB_TUPLE: u64 = 0;
pub const SUB_POS_BIG: u64 = 1;
pub const SUB_NEG_BIG: u64 = 2;
pub const SUB_REF: u64 = 3;
pub const SUB_FUN: u64 = 4;
pub const SUB_FLOAT: u64 = 5;
pub const SUB_BIN: u64 = 7;
pub const SUB_MAP: u64 = 8;
pub const SUB_MATCH: u64 = 9;

pub const SMALL_MAX: i64 = (1 << 59) - 1;
pub const SMALL_MIN: i64 = -(1 << 59);

#[inline]
pub fn small(v: i64) -> Term {
    ((v << 4) as u64) | 0xF
}
#[inline]
pub fn is_small(t: Term) -> bool {
    t & 0xF == 0xF
}
#[inline]
pub fn small_val(t: Term) -> i64 {
    (t as i64) >> 4
}
#[inline]
pub fn fits_small(v: i64) -> bool {
    (SMALL_MIN..=SMALL_MAX).contains(&v)
}
#[inline]
pub fn atom(i: u32) -> Term {
    ((i as u64) << 6) | 0x0B
}
#[inline]
pub fn is_atom(t: Term) -> bool {
    t & 0x3F == 0x0B
}
#[inline]
pub fn atom_index(t: Term) -> u32 {
    (t >> 6) as u32
}
#[inline]
pub fn pid(i: u64) -> Term {
    (i << 4) | 0x3
}
#[inline]
pub fn pid_val(t: Term) -> u32 {
    (t >> 4) as u32
}
/// Atom index of a pid's node (0 = this node). Bits 36.. of the pid word.
#[inline]
pub fn pid_node_idx(t: Term) -> u32 {
    (t >> 36) as u32
}
#[inline]
pub fn remote_pid(node_atom_idx: u32, id: u32) -> Term {
    ((((node_atom_idx as u64) << 32) | id as u64) << 4) | 0x3
}
#[inline]
pub fn is_pid(t: Term) -> bool {
    t & 0xF == 0x3
}
#[inline]
pub fn is_port(t: Term) -> bool {
    t & 0xF == 0x7
}
#[inline]
pub fn is_list(t: Term) -> bool {
    t & TAG_MASK == TAG_LIST
}
#[inline]
pub fn is_boxed(t: Term) -> bool {
    t & TAG_MASK == TAG_BOXED
}
#[inline]
pub fn is_immed(t: Term) -> bool {
    t & TAG_MASK == TAG_IMMED
}
#[inline]
pub fn ptr(t: Term) -> *mut u64 {
    (t & !TAG_MASK) as u32 as usize as *mut u64
}
#[inline]
pub fn make_boxed(p: *mut u64) -> Term {
    (p as usize as u64) | TAG_BOXED
}
#[inline]
pub fn make_list(p: *mut u64) -> Term {
    (p as usize as u64) | TAG_LIST
}
#[inline]
pub fn header(arity: u64, sub: u64) -> u64 {
    (arity << 6) | (sub << 2)
}
#[inline]
pub fn hdr_arity(h: u64) -> u64 {
    h >> 6
}
#[inline]
pub fn hdr_sub(h: u64) -> u64 {
    (h >> 2) & 0xF
}

/// Header word of a boxed term.
#[inline]
pub unsafe fn boxed_header(t: Term) -> u64 {
    *ptr(t)
}
#[inline]
pub fn boxed_sub(t: Term) -> Option<u64> {
    if is_boxed(t) {
        Some(hdr_sub(unsafe { boxed_header(t) }))
    } else {
        None
    }
}
#[inline]
pub fn is_tuple(t: Term) -> bool {
    boxed_sub(t) == Some(SUB_TUPLE)
}
#[inline]
pub fn is_float(t: Term) -> bool {
    boxed_sub(t) == Some(SUB_FLOAT)
}
#[inline]
pub fn is_big(t: Term) -> bool {
    matches!(boxed_sub(t), Some(SUB_POS_BIG) | Some(SUB_NEG_BIG))
}
#[inline]
pub fn is_integer(t: Term) -> bool {
    is_small(t) || is_big(t)
}
#[inline]
pub fn is_number(t: Term) -> bool {
    is_integer(t) || is_float(t)
}
#[inline]
pub fn is_binary_obj(t: Term) -> bool {
    boxed_sub(t) == Some(SUB_BIN)
}
#[inline]
pub fn is_map(t: Term) -> bool {
    boxed_sub(t) == Some(SUB_MAP)
}
#[inline]
pub fn is_fun(t: Term) -> bool {
    boxed_sub(t) == Some(SUB_FUN)
}
#[inline]
pub fn is_ref(t: Term) -> bool {
    boxed_sub(t) == Some(SUB_REF)
}

/// Tuple element, 0-based.
#[inline]
pub unsafe fn tuple_get(t: Term, i: usize) -> Term {
    *ptr(t).add(1 + i)
}
#[inline]
pub unsafe fn tuple_arity(t: Term) -> usize {
    hdr_arity(boxed_header(t)) as usize
}
#[inline]
pub unsafe fn head(t: Term) -> Term {
    *ptr(t)
}
#[inline]
pub unsafe fn tail(t: Term) -> Term {
    *ptr(t).add(1)
}
#[inline]
pub unsafe fn float_val(t: Term) -> f64 {
    f64::from_bits(*ptr(t).add(1))
}

/// Binary object: [header(3, BIN), bit_size, byte_ptr, bit_offset], all small integers.
pub struct Bin {
    pub bits: u64,
    pub base: *const u8,
    pub bitoff: u64,
}

impl Bin {
    pub fn bytes(&self) -> u64 {
        self.bits / 8
    }
    pub fn is_byte_aligned(&self) -> bool {
        self.bitoff == 0 && self.bits.is_multiple_of(8)
    }
    /// The whole binary as a byte slice. Only valid when byte aligned.
    pub unsafe fn slice(&self) -> &'static [u8] {
        core::slice::from_raw_parts(self.base, (self.bits / 8) as usize)
    }
    /// Bit `i` (0 = most significant bit of the first byte).
    pub unsafe fn bit(&self, i: u64) -> u8 {
        let abs = self.bitoff + i;
        (*self.base.add((abs / 8) as usize) >> (7 - abs % 8)) & 1
    }
    /// Copies the content into a byte vector; a trailing partial byte is left-aligned.
    pub unsafe fn to_vec(&self) -> Vec<u8> {
        if self.bitoff == 0 {
            let n = self.bits.div_ceil(8) as usize;
            let mut v = core::slice::from_raw_parts(self.base, n).to_vec();
            if !self.bits.is_multiple_of(8) {
                let keep = (self.bits % 8) as u32;
                let last = v.len() - 1;
                v[last] &= 0xFFu8 << (8 - keep);
            }
            v
        } else {
            let mut out = vec![0u8; self.bits.div_ceil(8) as usize];
            for i in 0..self.bits {
                if self.bit(i) == 1 {
                    out[(i / 8) as usize] |= 0x80 >> (i % 8);
                }
            }
            out
        }
    }
}

#[inline]
pub unsafe fn bin_of(t: Term) -> Bin {
    let p = ptr(t);
    Bin {
        bits: small_val(*p.add(1)) as u64,
        base: small_val(*p.add(2)) as u32 as usize as *const u8,
        bitoff: small_val(*p.add(3)) as u64,
    }
}

pub fn is_bitstring(t: Term) -> bool {
    is_binary_obj(t)
}

pub fn is_binary(t: Term) -> bool {
    is_binary_obj(t) && unsafe { small_val(*ptr(t).add(1)) % 8 == 0 }
}
