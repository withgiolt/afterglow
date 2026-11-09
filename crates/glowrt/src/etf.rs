//! External term format: term_to_binary / binary_to_term, matching what OTP 29 emits
//! for the term types tier 1 uses.

use crate::atoms;
use crate::build;
use crate::num;
use crate::term::*;
use num_bigint::Sign;

const SMALL_INTEGER: u8 = 97;
const INTEGER: u8 = 98;
const FLOAT_OLD: u8 = 99;
const NEW_FLOAT: u8 = 70;
const SMALL_BIG: u8 = 110;
const LARGE_BIG: u8 = 111;
const ATOM: u8 = 100;
const SMALL_ATOM: u8 = 115;
const ATOM_UTF8: u8 = 118;
const SMALL_ATOM_UTF8: u8 = 119;
const SMALL_TUPLE: u8 = 104;
const LARGE_TUPLE: u8 = 105;
const NIL_EXT: u8 = 106;
const STRING: u8 = 107;
const LIST: u8 = 108;
const BINARY: u8 = 109;
const BIT_BINARY: u8 = 77;
const MAP: u8 = 116;
const EXPORT: u8 = 113;
const NEW_PID: u8 = 88;
const NEWER_REF: u8 = 90;

pub struct Encoder {
    pub out: Vec<u8>,
}

impl Encoder {
    pub unsafe fn term(&mut self, t: Term) -> Result<(), ()> {
        let o = &mut self.out;
        if is_small(t) {
            let v = small_val(t);
            if (0..=255).contains(&v) {
                o.push(SMALL_INTEGER);
                o.push(v as u8);
            } else if v >= i32::MIN as i64 && v <= i32::MAX as i64 {
                o.push(INTEGER);
                o.extend_from_slice(&(v as i32).to_be_bytes());
            } else {
                self.bignum(t)?;
            }
            return Ok(());
        }
        if is_atom(t) {
            let name = atoms::name(t).as_bytes();
            if name.len() < 256 {
                o.push(SMALL_ATOM_UTF8);
                o.push(name.len() as u8);
            } else {
                o.push(ATOM_UTF8);
                o.extend_from_slice(&(name.len() as u16).to_be_bytes());
            }
            o.extend_from_slice(name);
            return Ok(());
        }
        if t == NIL {
            o.push(NIL_EXT);
            return Ok(());
        }
        if is_list(t) {
            // A proper list of bytes up to 65535 long is a STRING_EXT.
            let mut n = 0usize;
            let mut l = t;
            let mut bytes = true;
            while is_list(l) {
                let h = head(l);
                if !(is_small(h) && (0..=255).contains(&small_val(h))) {
                    bytes = false;
                }
                n += 1;
                l = tail(l);
            }
            if bytes && l == NIL && n <= 65535 {
                o.push(STRING);
                o.extend_from_slice(&(n as u16).to_be_bytes());
                let mut l = t;
                while is_list(l) {
                    self.out.push(small_val(head(l)) as u8);
                    l = tail(l);
                }
                return Ok(());
            }
            o.push(LIST);
            o.extend_from_slice(&(n as u32).to_be_bytes());
            let mut l = t;
            while is_list(l) {
                self.term(head(l))?;
                l = tail(l);
            }
            return self.term(l);
        }
        if is_pid(t) {
            o.push(NEW_PID);
            self.term(atoms::NONODE)?;
            let id = (t >> 4) as u32;
            self.out.extend_from_slice(&id.to_be_bytes());
            self.out.extend_from_slice(&0u32.to_be_bytes());
            self.out.extend_from_slice(&0u32.to_be_bytes());
            return Ok(());
        }
        match boxed_sub(t) {
            Some(SUB_TUPLE) => {
                let n = tuple_arity(t);
                if n < 256 {
                    o.push(SMALL_TUPLE);
                    o.push(n as u8);
                } else {
                    o.push(LARGE_TUPLE);
                    o.extend_from_slice(&(n as u32).to_be_bytes());
                }
                for i in 0..n {
                    self.term(tuple_get(t, i))?;
                }
                Ok(())
            }
            Some(SUB_FLOAT) => {
                o.push(NEW_FLOAT);
                o.extend_from_slice(&float_val(t).to_bits().to_be_bytes());
                Ok(())
            }
            Some(SUB_POS_BIG) | Some(SUB_NEG_BIG) => self.bignum(t),
            Some(SUB_BIN) => {
                let b = bin_of(t);
                let bytes = b.to_vec();
                if b.bits % 8 == 0 {
                    o.push(BINARY);
                    o.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
                } else {
                    o.push(BIT_BINARY);
                    o.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
                    o.push((b.bits % 8) as u8);
                }
                self.out.extend_from_slice(&bytes);
                Ok(())
            }
            Some(SUB_MAP) => {
                let n = build::map_size(t);
                o.push(MAP);
                o.extend_from_slice(&(n as u32).to_be_bytes());
                for i in 0..n {
                    self.term(build::map_key(t, i))?;
                    self.term(build::map_val(t, i))?;
                }
                Ok(())
            }
            Some(SUB_REF) => {
                o.push(NEWER_REF);
                o.extend_from_slice(&3u16.to_be_bytes());
                self.term(atoms::NONODE)?;
                self.out.extend_from_slice(&0u32.to_be_bytes());
                let id = *ptr(t).add(1);
                self.out.extend_from_slice(&((id & 0x3FFFF) as u32).to_be_bytes());
                self.out.extend_from_slice(&((id >> 18) as u32).to_be_bytes());
                self.out.extend_from_slice(&0u32.to_be_bytes());
                Ok(())
            }
            Some(SUB_FUN) => {
                let name = *ptr(t).add(4);
                if is_atom(name) {
                    o.push(EXPORT);
                    self.term(*ptr(t).add(3))?;
                    self.term(name)?;
                    self.term(*ptr(t).add(2))
                } else {
                    Err(())
                }
            }
            _ => Err(()),
        }
    }

    unsafe fn bignum(&mut self, t: Term) -> Result<(), ()> {
        let b = num::to_big(t).ok_or(())?;
        let (sign, bytes) = b.to_bytes_le();
        let o = &mut self.out;
        if bytes.len() < 256 {
            o.push(SMALL_BIG);
            o.push(bytes.len() as u8);
        } else {
            o.push(LARGE_BIG);
            o.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        }
        o.push(if sign == Sign::Minus { 1 } else { 0 });
        o.extend_from_slice(&bytes);
        Ok(())
    }
}

pub unsafe fn term_to_binary(t: Term) -> Option<Vec<u8>> {
    let mut e = Encoder { out: vec![131] };
    e.term(t).ok()?;
    Some(e.out)
}

pub struct Decoder<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    /// When set, binaries are references into `data` instead of copies.
    pub share: bool,
}

impl<'a> Decoder<'a> {
    fn u8(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return None;
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Some(s)
    }
    fn u16(&mut self) -> Option<usize> {
        let b = self.take(2)?;
        Some(u16::from_be_bytes([b[0], b[1]]) as usize)
    }
    fn u32(&mut self) -> Option<usize> {
        let b = self.take(4)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
    }

    unsafe fn atom_text(&mut self, n: usize) -> Option<Term> {
        let s = core::str::from_utf8(self.take(n)?).ok()?;
        Some(atoms::intern(s))
    }

    pub unsafe fn term(&mut self) -> Option<Term> {
        let tag = self.u8()?;
        match tag {
            SMALL_INTEGER => Some(small(self.u8()? as i64)),
            INTEGER => {
                let b = self.take(4)?;
                Some(small(i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as i64))
            }
            NEW_FLOAT => {
                let b = self.take(8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(b);
                Some(build::float(f64::from_bits(u64::from_be_bytes(a))))
            }
            FLOAT_OLD => {
                let s = core::str::from_utf8(self.take(31)?).ok()?;
                let f: f64 = s.trim_end_matches('\0').trim().parse().ok()?;
                Some(build::float(f))
            }
            SMALL_BIG | LARGE_BIG => {
                let n = if tag == SMALL_BIG { self.u8()? as usize } else { self.u32()? };
                let sign = self.u8()?;
                let bytes = self.take(n)?;
                let v = num_bigint::BigInt::from_bytes_le(
                    if sign == 0 { Sign::Plus } else { Sign::Minus },
                    bytes,
                );
                Some(build::big(&v))
            }
            ATOM_UTF8 | ATOM => {
                let n = self.u16()?;
                self.atom_text(n)
            }
            SMALL_ATOM_UTF8 | SMALL_ATOM => {
                let n = self.u8()? as usize;
                self.atom_text(n)
            }
            SMALL_TUPLE | LARGE_TUPLE => {
                let n = if tag == SMALL_TUPLE { self.u8()? as usize } else { self.u32()? };
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(self.term()?);
                }
                Some(build::tuple(&v))
            }
            NIL_EXT => Some(NIL),
            STRING => {
                let n = self.u16()?;
                let bytes = self.take(n)?;
                let items: Vec<Term> = bytes.iter().map(|&b| small(b as i64)).collect();
                Some(build::list(&items))
            }
            LIST => {
                let n = self.u32()?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(self.term()?);
                }
                let tail = self.term()?;
                Some(build::list_with_tail(&v, tail))
            }
            BINARY => {
                let n = self.u32()?;
                let bytes = self.take(n)?;
                if self.share {
                    Some(build::bin_ref(bytes.as_ptr(), 0, n as u64 * 8))
                } else {
                    Some(build::binary(bytes))
                }
            }
            BIT_BINARY => {
                let n = self.u32()?;
                let bits = self.u8()? as u64;
                let bytes = self.take(n)?;
                let total = if n == 0 { 0 } else { (n as u64 - 1) * 8 + bits };
                Some(build::bitstring(bytes, total))
            }
            MAP => {
                let n = self.u32()?;
                let mut pairs = Vec::with_capacity(n);
                for _ in 0..n {
                    let k = self.term()?;
                    let v = self.term()?;
                    pairs.push((k, v));
                }
                Some(build::map_from(pairs))
            }
            EXPORT => {
                let m = self.term()?;
                let f = self.term()?;
                let a = self.term()?;
                if !is_atom(m) || !is_atom(f) || !is_small(a) {
                    return None;
                }
                crate::funs::export_fun(m, f, small_val(a) as u32)
            }
            NEW_PID => {
                let _node = self.term()?;
                let id = self.u32()?;
                self.take(8)?;
                Some(pid(id as u64))
            }
            NEWER_REF => {
                let n = self.u16()?;
                let _node = self.term()?;
                self.take(4)?;
                let ids = self.take(4 * n)?;
                let a = u32::from_be_bytes([ids[0], ids[1], ids[2], ids[3]]) as u64;
                let b = if n > 1 { u32::from_be_bytes([ids[4], ids[5], ids[6], ids[7]]) as u64 } else { 0 };
                Some(crate::bifs::make_ref_with((b << 18) | a))
            }
            _ => None,
        }
    }
}

pub unsafe fn binary_to_term(bytes: &[u8], share: bool) -> Option<Term> {
    if bytes.first() != Some(&131) {
        return None;
    }
    let mut d = Decoder { data: bytes, pos: 1, share };
    let t = d.term()?;
    if d.pos != bytes.len() {
        return None;
    }
    Some(t)
}
