//! Building terms from Rust: tuples, lists, floats, binaries, integers, maps.

use crate::mem::{balloc, halloc};
use crate::term::*;
use num_bigint::{BigInt, Sign};

pub unsafe fn tuple(elems: &[Term]) -> Term {
    let p = halloc(1 + elems.len());
    *p = header(elems.len() as u64, SUB_TUPLE);
    for (i, e) in elems.iter().enumerate() {
        *p.add(1 + i) = *e;
    }
    make_boxed(p)
}

pub unsafe fn cons(h: Term, t: Term) -> Term {
    let p = halloc(2);
    *p = h;
    *p.add(1) = t;
    make_list(p)
}

pub unsafe fn list(items: &[Term]) -> Term {
    list_with_tail(items, NIL)
}

pub unsafe fn list_with_tail(items: &[Term], tail: Term) -> Term {
    if items.is_empty() {
        return tail;
    }
    let p = halloc(2 * items.len());
    for (i, it) in items.iter().enumerate() {
        *p.add(2 * i) = *it;
        *p.add(2 * i + 1) = if i + 1 < items.len() { make_list(p.add(2 * i + 2)) } else { tail };
    }
    make_list(p)
}

pub unsafe fn float(f: f64) -> Term {
    let p = halloc(2);
    *p = header(1, SUB_FLOAT);
    *p.add(1) = f.to_bits();
    make_boxed(p)
}

/// A binary object over existing bytes (not copied).
pub unsafe fn bin_ref(base: *const u8, bitoff: u64, bits: u64) -> Term {
    let p = halloc(4);
    *p = header(3, SUB_BIN);
    *p.add(1) = small(bits as i64);
    *p.add(2) = small(base as usize as i64);
    *p.add(3) = small(bitoff as i64);
    make_boxed(p)
}

/// A new binary holding a copy of `bytes`.
pub unsafe fn binary(bytes: &[u8]) -> Term {
    let b = balloc(bytes.len());
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), b, bytes.len());
    bin_ref(b, 0, bytes.len() as u64 * 8)
}

/// A new bitstring of `bits` bits from left-aligned `bytes`.
pub unsafe fn bitstring(bytes: &[u8], bits: u64) -> Term {
    let b = balloc(bytes.len());
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), b, bytes.len());
    bin_ref(b, 0, bits)
}

pub unsafe fn str_bin(s: &str) -> Term {
    binary(s.as_bytes())
}

pub unsafe fn int(v: i64) -> Term {
    if fits_small(v) {
        small(v)
    } else {
        big(&BigInt::from(v))
    }
}

pub unsafe fn uint(v: u64) -> Term {
    if v <= SMALL_MAX as u64 {
        small(v as i64)
    } else {
        big(&BigInt::from(v))
    }
}

/// Normalized: returns a small integer when it fits.
pub unsafe fn big(v: &BigInt) -> Term {
    if let Some(i) = num_traits::ToPrimitive::to_i64(v) {
        if fits_small(i) {
            return small(i);
        }
    }
    let (sign, digits) = v.to_u64_digits();
    let p = halloc(1 + digits.len());
    *p = header(digits.len() as u64, if sign == Sign::Minus { SUB_NEG_BIG } else { SUB_POS_BIG });
    for (i, d) in digits.iter().enumerate() {
        *p.add(1 + i) = *d;
    }
    make_boxed(p)
}

/// Map from pairs already sorted by key in map order, without duplicates.
pub unsafe fn map_sorted(pairs: &[(Term, Term)]) -> Term {
    let p = halloc(2 + 2 * pairs.len());
    *p = header(1 + 2 * pairs.len() as u64, SUB_MAP);
    *p.add(1) = small(pairs.len() as i64);
    for (i, (k, v)) in pairs.iter().enumerate() {
        *p.add(2 + 2 * i) = *k;
        *p.add(3 + 2 * i) = *v;
    }
    make_boxed(p)
}

pub unsafe fn map_size(m: Term) -> usize {
    small_val(*ptr(m).add(1)) as usize
}
pub unsafe fn map_key(m: Term, i: usize) -> Term {
    *ptr(m).add(2 + 2 * i)
}
pub unsafe fn map_val(m: Term, i: usize) -> Term {
    *ptr(m).add(3 + 2 * i)
}

/// Map from unsorted pairs; later duplicates win.
pub unsafe fn map_from(mut pairs: Vec<(Term, Term)>) -> Term {
    // Stable sort keeps insertion order among equal keys; keep the last.
    pairs.sort_by(|a, b| crate::cmp::map_key_cmp(a.0, b.0));
    let mut out: Vec<(Term, Term)> = Vec::with_capacity(pairs.len());
    for (k, v) in pairs {
        if let Some(last) = out.last_mut() {
            if crate::cmp::map_key_cmp(last.0, k) == core::cmp::Ordering::Equal {
                last.1 = v;
                continue;
            }
        }
        out.push((k, v));
    }
    map_sorted(&out)
}

pub unsafe fn list_to_vec(mut l: Term) -> Option<Vec<Term>> {
    let mut v = Vec::new();
    while is_list(l) {
        v.push(head(l));
        l = tail(l);
    }
    if l == NIL {
        Some(v)
    } else {
        None
    }
}
