//! Binaries: construction (bs_create_bin), matching (match contexts), and the
//! binary BIFs.
//!
//! Match context: [header(2, MATCH), binary term, small(position in bits)].
//! Flags passed by generated code: 1 little, 2 signed (native = little on wasm32).

use crate::atoms::*;
use crate::build;
use crate::exc::badarg;
use crate::mem::{balloc, halloc, ARENA};
use crate::term::*;
use num_bigint::{BigInt, Sign};

pub const F_LITTLE: i32 = 1;
pub const F_SIGNED: i32 = 2;

// ---------------------------------------------------------------------------
// Bit writer

pub struct Writer {
    pub buf: Vec<u8>,
    pub bits: u64,
}

impl Writer {
    pub fn new() -> Self {
        Writer { buf: Vec::new(), bits: 0 }
    }
    #[inline]
    pub fn push_byte(&mut self, b: u8) {
        if self.bits % 8 == 0 {
            self.buf.push(b);
        } else {
            let sh = (self.bits % 8) as u32;
            let last = self.buf.len() - 1;
            self.buf[last] |= b >> sh;
            self.buf.push(b << (8 - sh));
        }
        self.bits += 8;
    }
    pub fn push_bit(&mut self, bit: u8) {
        if self.bits % 8 == 0 {
            self.buf.push(0);
        }
        if bit != 0 {
            let last = self.buf.len() - 1;
            self.buf[last] |= 0x80 >> (self.bits % 8);
        }
        self.bits += 1;
    }
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        if self.bits % 8 == 0 {
            self.buf.extend_from_slice(bytes);
            self.bits += 8 * bytes.len() as u64;
        } else {
            for &b in bytes {
                self.push_byte(b);
            }
        }
    }
    /// Pushes the low `n` bits of a big-endian byte string (left-padded as needed).
    pub fn push_bits_of(&mut self, be: &[u8], n: u64) {
        // be holds at least ceil(n/8) bytes; the value is right-aligned.
        let total = be.len() as u64 * 8;
        let start = total - n;
        if start % 8 == 0 {
            self.push_bytes(&be[(start / 8) as usize..]);
            return;
        }
        for i in start..total {
            self.push_bit((be[(i / 8) as usize] >> (7 - i % 8)) & 1);
        }
    }
    pub unsafe fn push_bin(&mut self, b: &Bin, bits: u64) {
        if b.bitoff == 0 && bits % 8 == 0 {
            self.push_bytes(core::slice::from_raw_parts(b.base, (bits / 8) as usize));
        } else {
            let full = bits / 8;
            for i in 0..full {
                let mut v = 0u8;
                for k in 0..8 {
                    v = (v << 1) | b.bit(i * 8 + k);
                }
                self.push_byte(v);
            }
            for i in full * 8..bits {
                self.push_bit(b.bit(i));
            }
        }
    }
    pub unsafe fn finish(self) -> Term {
        build::bitstring(&self.buf, self.bits)
    }
}

/// Integer `v` as `n` bits, two's complement, big- or little-endian.
pub fn int_bits(v: &BigInt, n: u64, little: bool) -> Vec<u8> {
    let nbytes = ((n + 7) / 8) as usize;
    let mut le = v.to_signed_bytes_le();
    let fill = if v.sign() == Sign::Minus { 0xFF } else { 0 };
    le.resize(nbytes.max(le.len()), fill);
    le.truncate(nbytes);
    if little {
        // Little-endian of a non-multiple-of-8 size: ERTS writes the low bytes first and
        // the last partial byte holds the top bits.
        if n % 8 == 0 {
            return le; // caller pushes as whole bytes
        }
    }
    let mut be: Vec<u8> = le.into_iter().rev().collect();
    if n % 8 != 0 {
        let extra = 8 - (n % 8) as u32;
        be[0] &= 0xFFu8 >> extra;
    }
    be
}

unsafe fn int_of(t: Term) -> Option<BigInt> {
    if is_small(t) {
        Some(BigInt::from(small_val(t)))
    } else if is_big(t) {
        crate::num::to_big(t)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// bs_create_bin

const T_INTEGER: u64 = 0;
const T_BINARY: u64 = 1;
const T_FLOAT: u64 = 2;
const T_UTF8: u64 = 4;
const T_UTF16: u64 = 5;
const T_UTF32: u64 = 6;
const T_APPEND: u64 = 7;
const T_PRIVATE_APPEND: u64 = 8;

/// Size operand: `all` -> None; integer -> Some(n); anything else -> Err.
unsafe fn seg_size(t: Term) -> Result<Option<u64>, ()> {
    if t == ALL_ATOM() {
        return Ok(None);
    }
    if is_small(t) && small_val(t) >= 0 {
        return Ok(Some(small_val(t) as u64));
    }
    Err(())
}

#[allow(non_snake_case)]
fn ALL_ATOM() -> Term {
    ALL
}

pub fn utf8_encode(c: u32, w: &mut Writer) -> bool {
    match char::from_u32(c) {
        Some(ch) => {
            let mut b = [0u8; 4];
            w.push_bytes(ch.encode_utf8(&mut b).as_bytes());
            true
        }
        None => false,
    }
}

/// Builds a binary from `n` segments stored at `segs` as 5 words each:
/// type, unit, flags, value term, size term. Returns 0 with badarg raised on error.
#[no_mangle]
pub unsafe extern "C" fn rt_bs_create(n: u32, segs: u32) -> Term {
    let s = core::slice::from_raw_parts(segs as usize as *const u64, 5 * n as usize);
    match create(s) {
        Some(t) => t,
        None => badarg(),
    }
}

unsafe fn create(s: &[u64]) -> Option<Term> {
    let nseg = s.len() / 5;
    let mut w = Writer::new();
    let mut k = 0;
    // Fast append: extend the first segment's bytes in place when they end at the
    // arena's top.
    let mut in_place: Option<(*const u8, u64)> = None;
    if nseg > 0 && (s[0] == T_APPEND || s[0] == T_PRIVATE_APPEND) {
        let v = s[3];
        if !is_bitstring(v) {
            return None;
        }
        let b = bin_of(v);
        let unit = s[1].max(1);
        if b.bits % unit != 0 {
            return None;
        }
        if b.bitoff == 0 && b.bits % 8 == 0 {
            in_place = Some((b.base, b.bits));
        } else {
            w.push_bin(&b, b.bits);
        }
        k = 1;
    }
    for i in k..nseg {
        let (ty, unit, flags, v, size) = (s[5 * i], s[5 * i + 1], s[5 * i + 2] as i32, s[5 * i + 3], s[5 * i + 4]);
        let little = flags & F_LITTLE != 0;
        match ty {
            T_INTEGER => {
                let n = seg_size(size).ok()?? * unit.max(1);
                let val = int_of(v)?;
                if n == 0 {
                    continue;
                }
                if is_small(v) && n <= 64 && !little {
                    let x = small_val(v) as u64;
                    let be = x.to_be_bytes();
                    if n % 8 == 0 {
                        w.push_bytes(&be[8 - (n / 8) as usize..]);
                    } else {
                        let masked = if n == 64 { x } else { x & ((1u64 << n) - 1) };
                        w.push_bits_of(&masked.to_be_bytes(), n);
                    }
                    continue;
                }
                let bytes = int_bits(&val, n, little);
                if little && n % 8 == 0 {
                    w.push_bytes(&bytes);
                } else if little {
                    // whole bytes little-endian, then the remaining high bits
                    let le = {
                        let mut le = val.to_signed_bytes_le();
                        let fill = if val.sign() == Sign::Minus { 0xFF } else { 0 };
                        le.resize(((n + 7) / 8) as usize, fill);
                        le
                    };
                    let full = (n / 8) as usize;
                    w.push_bytes(&le[..full]);
                    let rem = n % 8;
                    w.push_bits_of(&[le[full] & ((1u8 << rem) - 1)], rem);
                } else {
                    w.push_bits_of(&bytes, n);
                }
            }
            T_BINARY => {
                if !is_bitstring(v) {
                    return None;
                }
                let b = bin_of(v);
                let bits = match seg_size(size).ok()? {
                    None => {
                        if b.bits % unit.max(1) != 0 {
                            return None;
                        }
                        b.bits
                    }
                    Some(n) => {
                        let n = n * unit.max(1);
                        if n > b.bits {
                            return None;
                        }
                        n
                    }
                };
                w.push_bin(&b, bits);
            }
            T_FLOAT => {
                let n = seg_size(size).ok()?? * unit.max(1);
                let f = crate::num::to_f64(v)?;
                let bytes: Vec<u8> = match n {
                    64 => f.to_be_bytes().to_vec(),
                    32 => (f as f32).to_be_bytes().to_vec(),
                    16 => half_bits(f)?.to_be_bytes().to_vec(),
                    _ => return None,
                };
                if little {
                    let mut r = bytes;
                    r.reverse();
                    w.push_bytes(&r);
                } else {
                    w.push_bytes(&bytes);
                }
            }
            T_UTF8 => {
                if !is_small(v) || !utf8_encode(small_val(v) as u32, &mut w) {
                    return None;
                }
            }
            T_UTF16 => {
                if !is_small(v) {
                    return None;
                }
                let c = char::from_u32(small_val(v) as u32)?;
                let mut b = [0u16; 2];
                for u in c.encode_utf16(&mut b) {
                    let bytes = if little { u.to_le_bytes() } else { u.to_be_bytes() };
                    w.push_bytes(&bytes);
                }
            }
            T_UTF32 => {
                if !is_small(v) {
                    return None;
                }
                let c = char::from_u32(small_val(v) as u32)? as u32;
                w.push_bytes(&if little { c.to_le_bytes() } else { c.to_be_bytes() });
            }
            _ => return None,
        }
    }
    if let Some((base, bits)) = in_place {
        let old = (bits / 8) as usize;
        let add = w.buf.len();
        let arena = &mut *core::ptr::addr_of_mut!(ARENA);
        if w.bits % 8 == 0 && arena.try_extend(base, old, old + add) {
            core::ptr::copy_nonoverlapping(w.buf.as_ptr(), base.add(old) as *mut u8, add);
            return Some(build::bin_ref(base, 0, bits + w.bits));
        }
        // Copy: old bytes, then the new bits. Leave room for further appends.
        let total_bytes = old + add;
        let cap = (total_bytes * 2).max(64);
        let nb = balloc(cap);
        core::ptr::copy_nonoverlapping(base, nb, old);
        core::ptr::copy_nonoverlapping(w.buf.as_ptr(), nb.add(old), add);
        // Shrink the arena reservation to the used size so try_extend can grow it.
        shrink_last(nb, cap, total_bytes);
        return Some(build::bin_ref(nb, 0, bits + w.bits));
    }
    Some(w.finish())
}

/// Gives back the unused tail of the most recent arena allocation.
unsafe fn shrink_last(p: *mut u8, cap: usize, used: usize) {
    let arena = &mut *core::ptr::addr_of_mut!(ARENA);
    // try_extend from `cap` to `used` moves pos back when p is the last allocation.
    arena.try_extend(p, cap, used);
}

fn half_bits(f: f64) -> Option<u16> {
    // IEEE 754 binary16, round to nearest even; overflow is an error.
    let bits = (f as f32).to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xFF) as i32 - 127 + 15;
    let mant = bits & 0x7F_FFFF;
    if f.is_nan() || f.is_infinite() {
        return None;
    }
    if exp >= 31 {
        return None;
    }
    if exp <= 0 {
        if exp < -10 {
            return Some(sign);
        }
        let m = (mant | 0x80_0000) >> (1 - exp);
        let r = (m + 0x1000 + ((m >> 13) & 1) - 1) >> 13;
        return Some(sign | r as u16);
    }
    let r = ((exp as u32) << 10) | (mant >> 13);
    let rem = mant & 0x1FFF;
    let r = if rem > 0x1000 || (rem == 0x1000 && r & 1 == 1) { r + 1 } else { r };
    if r >= 0x7C00 {
        return None;
    }
    Some(sign | r as u16)
}

#[no_mangle]
pub unsafe extern "C" fn rt_bs_init_writable(_size: Term) -> Term {
    let b = balloc(64);
    shrink_last(b, 64, 0);
    build::bin_ref(b, 0, 0)
}

// ---------------------------------------------------------------------------
// Matching

#[inline]
unsafe fn ctx_bin(ctx: Term) -> Bin {
    bin_of(*ptr(ctx).add(1))
}
#[inline]
unsafe fn ctx_pos(ctx: Term) -> u64 {
    small_val(*ptr(ctx).add(2)) as u64
}
#[inline]
unsafe fn set_pos(ctx: Term, p: u64) {
    *ptr(ctx).add(2) = small(p as i64);
}
#[inline]
unsafe fn remaining(ctx: Term) -> u64 {
    ctx_bin(ctx).bits - ctx_pos(ctx)
}

/// Starts a match. Returns the context, or 0 when `src` is not a bitstring (with
/// badarg raised when `raise` is set).
#[no_mangle]
pub unsafe extern "C" fn rt_bs_start(src: Term, raise: i32) -> Term {
    if boxed_sub(src) == Some(SUB_MATCH) {
        return src;
    }
    if !is_bitstring(src) {
        if raise != 0 {
            return badarg();
        }
        return 0;
    }
    let p = halloc(3);
    *p = header(2, SUB_MATCH);
    *p.add(1) = src;
    *p.add(2) = small(0);
    make_boxed(p)
}

#[no_mangle]
pub unsafe extern "C" fn rt_bs_ensure(ctx: Term, bits: i64, unit: i64) -> i32 {
    let r = remaining(ctx) as i64;
    (r >= bits && (unit <= 1 || (r - bits) % unit == 0)) as i32
}
#[no_mangle]
pub unsafe extern "C" fn rt_bs_ensure_exactly(ctx: Term, bits: i64) -> i32 {
    (remaining(ctx) as i64 == bits) as i32
}
#[no_mangle]
pub unsafe extern "C" fn rt_bs_test_unit(ctx: Term, unit: i64) -> i32 {
    (unit <= 1 || remaining(ctx) as i64 % unit == 0) as i32
}

/// Reads `n` ≤ 64 bits big-endian at the current position, without advancing.
unsafe fn peek_u64(b: &Bin, pos: u64, n: u64) -> u64 {
    let abs = b.bitoff + pos;
    if abs % 8 == 0 && n % 8 == 0 {
        let p = b.base.add((abs / 8) as usize);
        let mut v = 0u64;
        for i in 0..(n / 8) as usize {
            v = (v << 8) | *p.add(i) as u64;
        }
        return v;
    }
    let mut v = 0u64;
    for i in 0..n {
        v = (v << 1) | b.bit(pos + i) as u64;
    }
    v
}

/// Reads `n` bits as bytes (big-endian, right-aligned in the result).
unsafe fn peek_bytes(b: &Bin, pos: u64, n: u64) -> Vec<u8> {
    let nbytes = ((n + 7) / 8) as usize;
    let mut out = vec![0u8; nbytes];
    let lead = nbytes as u64 * 8 - n;
    for i in 0..n {
        if b.bit(pos + i) == 1 {
            let o = lead + i;
            out[(o / 8) as usize] |= 0x80 >> (o % 8);
        }
    }
    out
}

/// `=:=` command: compares the next `bits` with integer `value`; advances on success.
#[no_mangle]
pub unsafe extern "C" fn rt_bs_eq(ctx: Term, bits: i64, value: Term) -> i32 {
    let b = ctx_bin(ctx);
    let pos = ctx_pos(ctx);
    if (b.bits - pos) < bits as u64 {
        return 0;
    }
    let ok = if bits <= 64 && is_small(value) {
        let v = peek_u64(&b, pos, bits as u64);
        let want = small_val(value) as u64;
        let mask = if bits == 64 { u64::MAX } else { (1u64 << bits) - 1 };
        v == want & mask && (small_val(value) >= 0 || bits == 64)
    } else {
        let got = BigInt::from_bytes_be(Sign::Plus, &peek_bytes(&b, pos, bits as u64));
        Some(got) == int_of(value)
    };
    if ok {
        set_pos(ctx, pos + bits as u64);
    }
    ok as i32
}

unsafe fn read_int(ctx: Term, n: u64, flags: i32) -> Term {
    let b = ctx_bin(ctx);
    let pos = ctx_pos(ctx);
    set_pos(ctx, pos + n);
    let little = flags & F_LITTLE != 0;
    let signed = flags & F_SIGNED != 0;
    if n == 0 {
        return small(0);
    }
    if n <= 64 && !little {
        let v = peek_u64(&b, pos, n);
        if signed {
            let sh = 64 - n as u32;
            return build::int(((v << sh) as i64) >> sh);
        }
        return build::uint(v);
    }
    let mut bytes = peek_bytes(&b, pos, n);
    if little {
        // Little-endian: bytes as read are the low bytes first. For sizes that are not a
        // multiple of 8 the trailing partial byte holds the most significant bits.
        let rem = n % 8;
        let mut le: Vec<u8> = Vec::new();
        if rem == 0 {
            le = bytes.clone();
        } else {
            // re-read: full bytes first, then `rem` bits
            let full = n / 8;
            for i in 0..full {
                le.push(peek_u64(&b, pos + 8 * i, 8) as u8);
            }
            le.push(peek_u64(&b, pos + 8 * full, rem) as u8);
        }
        bytes = le.into_iter().rev().collect();
    }
    let mut v = BigInt::from_bytes_be(Sign::Plus, &bytes);
    if signed && v.bit(n - 1) {
        v -= BigInt::from(1) << n;
    }
    build::big(&v)
}

unsafe fn read_bin(ctx: Term, n: u64) -> Term {
    let b = ctx_bin(ctx);
    let pos = ctx_pos(ctx);
    set_pos(ctx, pos + n);
    let abs = b.bitoff + pos;
    build::bin_ref(b.base.add((abs / 8) as usize), abs % 8, n)
}

#[no_mangle]
pub unsafe extern "C" fn rt_bs_int(ctx: Term, bits: i64, flags: i32) -> Term {
    read_int(ctx, bits as u64, flags)
}
#[no_mangle]
pub unsafe extern "C" fn rt_bs_bin(ctx: Term, bits: i64) -> Term {
    read_bin(ctx, bits as u64)
}
/// bs_match's get_tail command: the rest, consumed.
#[no_mangle]
pub unsafe extern "C" fn rt_bs_tail(ctx: Term) -> Term {
    let r = remaining(ctx);
    read_bin(ctx, r)
}
/// The bs_get_tail instruction: the rest, without moving the position.
#[no_mangle]
pub unsafe extern "C" fn rt_bs_peek_tail(ctx: Term) -> Term {
    let pos = ctx_pos(ctx);
    let r = remaining(ctx);
    let t = read_bin(ctx, r);
    set_pos(ctx, pos);
    t
}
#[no_mangle]
pub unsafe extern "C" fn rt_bs_skip(ctx: Term, bits: i64) {
    set_pos(ctx, ctx_pos(ctx) + bits as u64);
}
#[no_mangle]
pub unsafe extern "C" fn rt_bs_get_position(ctx: Term) -> Term {
    small(ctx_pos(ctx) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn rt_bs_set_position(ctx: Term, pos: Term) {
    set_pos(ctx, small_val(pos) as u64);
}

/// Size operand of the older match instructions: integer × unit, or `all`.
unsafe fn size_bits(ctx: Term, size: Term, unit: i64) -> Option<u64> {
    if size == ALL {
        let r = remaining(ctx);
        if unit > 1 && r % unit as u64 != 0 {
            return None;
        }
        return Some(r);
    }
    if !is_small(size) || small_val(size) < 0 {
        return None;
    }
    Some(small_val(size) as u64 * unit.max(1) as u64)
}

#[no_mangle]
pub unsafe extern "C" fn rt_bs_get_integer2(ctx: Term, size: Term, unit: i64, flags: i32) -> Term {
    match size_bits(ctx, size, unit) {
        Some(n) if n <= remaining(ctx) => read_int(ctx, n, flags),
        _ => 0,
    }
}
#[no_mangle]
pub unsafe extern "C" fn rt_bs_get_binary2(ctx: Term, size: Term, unit: i64) -> Term {
    match size_bits(ctx, size, unit) {
        Some(n) if n <= remaining(ctx) => read_bin(ctx, n),
        _ => 0,
    }
}
#[no_mangle]
pub unsafe extern "C" fn rt_bs_get_float2(ctx: Term, size: Term, unit: i64, flags: i32) -> Term {
    let Some(n) = size_bits(ctx, size, unit) else { return 0 };
    if n > remaining(ctx) || !(n == 64 || n == 32 || n == 16) {
        return 0;
    }
    let b = ctx_bin(ctx);
    let pos = ctx_pos(ctx);
    let mut v = peek_u64(&b, pos, n);
    if flags & F_LITTLE != 0 {
        v = match n {
            64 => v.swap_bytes(),
            32 => (v as u32).swap_bytes() as u64,
            _ => (v as u16).swap_bytes() as u64,
        };
    }
    let f = match n {
        64 => f64::from_bits(v),
        32 => f32::from_bits(v as u32) as f64,
        _ => half_to_f64(v as u16),
    };
    if !f.is_finite() {
        return 0;
    }
    set_pos(ctx, pos + n);
    build::float(f)
}

fn half_to_f64(h: u16) -> f64 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1F) as i32;
    let mant = (h & 0x3FF) as f64;
    if exp == 0 {
        sign * mant * 2f64.powi(-24)
    } else if exp == 31 {
        f64::NAN
    } else {
        sign * (1.0 + mant / 1024.0) * 2f64.powi(exp - 15)
    }
}

#[no_mangle]
pub unsafe extern "C" fn rt_bs_skip_bits2(ctx: Term, size: Term, unit: i64) -> i32 {
    match size_bits(ctx, size, unit) {
        Some(n) if n <= remaining(ctx) => {
            set_pos(ctx, ctx_pos(ctx) + n);
            1
        }
        _ => 0,
    }
}

/// bs_match_string: compares the next `bits` with the bits of `lit` (a binary).
#[no_mangle]
pub unsafe extern "C" fn rt_bs_match_string(ctx: Term, bits: i64, lit: Term) -> i32 {
    let bits = bits as u64;
    if remaining(ctx) < bits {
        return 0;
    }
    let b = ctx_bin(ctx);
    let pos = ctx_pos(ctx);
    let l = bin_of(lit);
    let abs = b.bitoff + pos;
    let ok = if abs % 8 == 0 && bits % 8 == 0 {
        let a = core::slice::from_raw_parts(b.base.add((abs / 8) as usize), (bits / 8) as usize);
        a == &l.slice()[..(bits / 8) as usize]
    } else {
        (0..bits).all(|i| b.bit(pos + i) == l.bit(i))
    };
    if ok {
        set_pos(ctx, pos + bits);
    }
    ok as i32
}

/// Decodes one UTF-8 code point. Returns (code point, bits) or None.
unsafe fn utf8_at(ctx: Term) -> Option<(u32, u64)> {
    let b = ctx_bin(ctx);
    let pos = ctx_pos(ctx);
    let rem = b.bits - pos;
    if rem < 8 {
        return None;
    }
    let byte = |i: u64| peek_u64(&b, pos + 8 * i, 8) as u32;
    let b0 = byte(0);
    let (len, init) = match b0 {
        0x00..=0x7F => return Some((b0, 8)),
        0xC2..=0xDF => (2, b0 & 0x1F),
        0xE0..=0xEF => (3, b0 & 0x0F),
        0xF0..=0xF4 => (4, b0 & 0x07),
        _ => return None,
    };
    if rem < 8 * len as u64 {
        return None;
    }
    let mut c = init;
    for i in 1..len {
        let x = byte(i);
        if x & 0xC0 != 0x80 {
            return None;
        }
        c = (c << 6) | (x & 0x3F);
    }
    let min = [0, 0, 0x80, 0x800, 0x10000][len as usize];
    if c < min || c > 0x10FFFF || (0xD800..=0xDFFF).contains(&c) {
        return None;
    }
    Some((c, 8 * len as u64))
}

unsafe fn utf_at(ctx: Term, kind: i32, flags: i32) -> Option<(u32, u64)> {
    let little = flags & F_LITTLE != 0;
    match kind {
        8 => utf8_at(ctx),
        16 => {
            let b = ctx_bin(ctx);
            let pos = ctx_pos(ctx);
            let rem = b.bits - pos;
            let rd = |off: u64| {
                let v = peek_u64(&b, pos + off, 16) as u16;
                if little { v.swap_bytes() } else { v }
            };
            if rem < 16 {
                return None;
            }
            let w1 = rd(0) as u32;
            if (0xD800..0xDC00).contains(&w1) {
                if rem < 32 {
                    return None;
                }
                let w2 = rd(16) as u32;
                if !(0xDC00..0xE000).contains(&w2) {
                    return None;
                }
                return Some((0x10000 + ((w1 - 0xD800) << 10) + (w2 - 0xDC00), 32));
            }
            if (0xDC00..0xE000).contains(&w1) {
                return None;
            }
            Some((w1, 16))
        }
        _ => {
            let b = ctx_bin(ctx);
            let pos = ctx_pos(ctx);
            if b.bits - pos < 32 {
                return None;
            }
            let mut v = peek_u64(&b, pos, 32) as u32;
            if little {
                v = v.swap_bytes();
            }
            if v > 0x10FFFF || (0xD800..=0xDFFF).contains(&v) {
                return None;
            }
            Some((v, 32))
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rt_bs_get_utf(ctx: Term, kind: i32, flags: i32) -> Term {
    match utf_at(ctx, kind, flags) {
        Some((c, n)) => {
            set_pos(ctx, ctx_pos(ctx) + n);
            small(c as i64)
        }
        None => 0,
    }
}
#[no_mangle]
pub unsafe extern "C" fn rt_bs_skip_utf(ctx: Term, kind: i32, flags: i32) -> i32 {
    match utf_at(ctx, kind, flags) {
        Some((_, n)) => {
            set_pos(ctx, ctx_pos(ctx) + n);
            1
        }
        None => 0,
    }
}

// ---------------------------------------------------------------------------
// BIFs

macro_rules! bif {
    ($name:literal, fn $f:ident($($a:ident),*) $body:block) => {
        #[export_name = $name]
        pub unsafe extern "C" fn $f($($a: Term),*) -> Term $body
    };
}

bif!("erlang:byte_size/1", fn byte_size(b) {
    if boxed_sub(b) == Some(SUB_MATCH) { return small(((remaining(b) + 7) / 8) as i64); }
    if !is_bitstring(b) { return badarg(); }
    small(((bin_of(b).bits + 7) / 8) as i64)
});
bif!("erlang:bit_size/1", fn bit_size(b) {
    if boxed_sub(b) == Some(SUB_MATCH) { return small(remaining(b) as i64); }
    if !is_bitstring(b) { return badarg(); }
    small(bin_of(b).bits as i64)
});

unsafe fn part(b: Term, pos: Term, len: Term) -> Term {
    if !is_binary(b) || !is_small(pos) || !is_small(len) {
        return badarg();
    }
    let bin = bin_of(b);
    let size = (bin.bits / 8) as i64;
    let (mut s, mut l) = (small_val(pos), small_val(len));
    if l < 0 {
        s += l;
        l = -l;
    }
    if s < 0 || s + l > size {
        return badarg();
    }
    let abs = bin.bitoff + 8 * s as u64;
    build::bin_ref(bin.base.add((abs / 8) as usize), abs % 8, 8 * l as u64)
}
bif!("erlang:binary_part/3", fn binary_part3(b, p, l) { part(b, p, l) });
bif!("erlang:binary_part/2", fn binary_part2(b, pl) {
    if !is_tuple(pl) || tuple_arity(pl) != 2 { return badarg(); }
    part(b, tuple_get(pl, 0), tuple_get(pl, 1))
});
bif!("binary:part/3", fn binary_part3b(b, p, l) { part(b, p, l) });
bif!("binary:part/2", fn binary_part2b(b, pl) { binary_part2(b, pl) });

bif!("erlang:binary_to_list/1", fn binary_to_list(b) {
    if !is_binary(b) { return badarg(); }
    let v: Vec<Term> = bin_of(b).to_vec().iter().map(|&c| small(c as i64)).collect();
    build::list(&v)
});
bif!("erlang:binary_to_list/3", fn binary_to_list3(b, s, e) {
    if !is_binary(b) || !is_small(s) || !is_small(e) { return badarg(); }
    let v = bin_of(b).to_vec();
    let (s, e) = (small_val(s), small_val(e));
    if s < 1 || e < s || e as usize > v.len() { return badarg(); }
    let l: Vec<Term> = v[(s - 1) as usize..e as usize].iter().map(|&c| small(c as i64)).collect();
    build::list(&l)
});
bif!("erlang:bitstring_to_list/1", fn bitstring_to_list(b) {
    if !is_bitstring(b) { return badarg(); }
    let bin = bin_of(b);
    let full = bin.bits / 8;
    let v = bin.to_vec();
    let items: Vec<Term> = v[..full as usize].iter().map(|&c| small(c as i64)).collect();
    if bin.bits % 8 == 0 {
        return build::list(&items);
    }
    let rest = build::bitstring(&v[full as usize..], bin.bits % 8);
    build::list_with_tail(&items, build::cons(rest, NIL)) // [B1,...,Bits]
});

/// Flattens an iolist (bytes, binaries, nested lists, binary tail) into a writer.
pub unsafe fn iolist(t: Term, w: &mut Writer, allow_bits: bool) -> bool {
    let mut stack = vec![t];
    while let Some(t) = stack.pop() {
        if t == NIL {
            continue;
        }
        if is_bitstring(t) {
            let b = bin_of(t);
            if !allow_bits && b.bits % 8 != 0 {
                return false;
            }
            w.push_bin(&b, b.bits);
        } else if is_list(t) {
            let h = head(t);
            stack.push(tail(t));
            if is_small(h) {
                let c = small_val(h);
                if !(0..=255).contains(&c) {
                    return false;
                }
                w.push_byte(c as u8);
            } else if h == NIL || is_list(h) || is_bitstring(h) {
                stack.push(h);
            } else {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

bif!("erlang:iolist_to_binary/1", fn iolist_to_binary(t) {
    if is_binary(t) { return t; }
    let mut w = Writer::new();
    if !is_list(t) && t != NIL { return badarg(); }
    if !iolist(t, &mut w, false) { return badarg(); }
    w.finish()
});
bif!("erlang:list_to_binary/1", fn list_to_binary(t) {
    if !is_list(t) && t != NIL { return badarg(); }
    let mut w = Writer::new();
    if !iolist(t, &mut w, false) { return badarg(); }
    w.finish()
});
bif!("erlang:list_to_bitstring/1", fn list_to_bitstring(t) {
    if !is_list(t) && t != NIL { return badarg(); }
    let mut w = Writer::new();
    if !iolist(t, &mut w, true) { return badarg(); }
    w.finish()
});
bif!("erlang:iolist_size/1", fn iolist_size(t) {
    if is_binary(t) { return small((bin_of(t).bits / 8) as i64); }
    let mut w = Writer::new();
    if !iolist(t, &mut w, false) { return badarg(); }
    small((w.bits / 8) as i64)
});
bif!("erlang:split_binary/2", fn split_binary(b, n) {
    if !is_binary(b) || !is_small(n) { return badarg(); }
    let size = (bin_of(b).bits / 8) as i64;
    let k = small_val(n);
    if k < 0 || k > size { return badarg(); }
    let l = part(b, small(0), small(k));
    let r = part(b, small(k), small(size - k));
    build::tuple(&[l, r])
});
