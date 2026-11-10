//! Integer and float arithmetic: small integers fast, bignums through num-bigint,
//! floats boxed. Errors raise badarith.

use crate::build;
use crate::exc;
use crate::term::*;
use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_traits::{Signed, ToPrimitive, Zero};

pub enum Num {
    I(BigInt),
    F(f64),
}

pub unsafe fn to_big(t: Term) -> Option<BigInt> {
    if is_small(t) {
        return Some(BigInt::from(small_val(t)));
    }
    if is_big(t) {
        let p = ptr(t);
        let h = *p;
        let n = hdr_arity(h) as usize;
        let digits: Vec<u64> = (0..n).map(|i| *p.add(1 + i)).collect();
        let mut v = BigInt::from_slice(Sign::Plus, &[]);
        for d in digits.iter().rev() {
            v = (v << 64) + BigInt::from(*d);
        }
        if hdr_sub(h) == SUB_NEG_BIG {
            v = -v;
        }
        return Some(v);
    }
    None
}

pub unsafe fn to_num(t: Term) -> Option<Num> {
    if is_float(t) {
        Some(Num::F(float_val(t)))
    } else {
        to_big(t).map(Num::I)
    }
}

pub unsafe fn to_f64(t: Term) -> Option<f64> {
    if is_small(t) {
        Some(small_val(t) as f64)
    } else if is_float(t) {
        Some(float_val(t))
    } else {
        to_big(t).map(|b| big_to_f64(&b))
    }
}

pub fn big_to_f64(b: &BigInt) -> f64 {
    b.to_f64().unwrap_or(if b.is_negative() { f64::NEG_INFINITY } else { f64::INFINITY })
}

unsafe fn float_result(f: f64) -> Term {
    if f.is_finite() {
        build::float(f)
    } else {
        exc::error(crate::atoms::BADARITH)
    }
}

macro_rules! arith {
    ($name:ident, $int:expr, $flt:expr) => {
        pub unsafe fn $name(a: Term, b: Term) -> Term {
            match (to_num(a), to_num(b)) {
                (Some(Num::I(x)), Some(Num::I(y))) => build::big(&$int(x, y)),
                (Some(x), Some(y)) => {
                    let fx = match x { Num::I(i) => big_to_f64(&i), Num::F(f) => f };
                    let fy = match y { Num::I(i) => big_to_f64(&i), Num::F(f) => f };
                    float_result($flt(fx, fy))
                }
                _ => exc::error(crate::atoms::BADARITH),
            }
        }
    };
}

arith!(add, |x: BigInt, y: BigInt| x + y, |x: f64, y: f64| x + y);
arith!(sub, |x: BigInt, y: BigInt| x - y, |x: f64, y: f64| x - y);
arith!(mul, |x: BigInt, y: BigInt| x * y, |x: f64, y: f64| x * y);

#[inline]
pub unsafe fn add_fast(a: Term, b: Term) -> Term {
    if is_small(a) && is_small(b) {
        return build::int(small_val(a) + small_val(b));
    }
    add(a, b)
}

pub unsafe fn fdiv(a: Term, b: Term) -> Term {
    match (to_f64(a), to_f64(b)) {
        (Some(_), Some(y)) if y == 0.0 => exc::error(crate::atoms::BADARITH),
        (Some(x), Some(y)) if is_number(a) && is_number(b) => float_result(x / y),
        _ => exc::error(crate::atoms::BADARITH),
    }
}

unsafe fn int_op(a: Term, b: Term, f: impl Fn(BigInt, BigInt) -> Option<BigInt>) -> Term {
    match (to_big(a), to_big(b)) {
        (Some(x), Some(y)) => match f(x, y) {
            Some(r) => build::big(&r),
            None => exc::error(crate::atoms::BADARITH),
        },
        _ => exc::error(crate::atoms::BADARITH),
    }
}

pub unsafe fn div(a: Term, b: Term) -> Term {
    if is_small(a) && is_small(b) && small_val(b) != 0 {
        return build::int(small_val(a) / small_val(b));
    }
    int_op(a, b, |x, y| if y.is_zero() { None } else { Some(x / y) })
}

pub unsafe fn rem(a: Term, b: Term) -> Term {
    if is_small(a) && is_small(b) && small_val(b) != 0 {
        return build::int(small_val(a) % small_val(b));
    }
    int_op(a, b, |x, y| if y.is_zero() { None } else { Some(x % y) })
}

pub unsafe fn band(a: Term, b: Term) -> Term {
    if is_small(a) && is_small(b) {
        return small(small_val(a) & small_val(b));
    }
    int_op(a, b, |x, y| Some(x & y))
}
pub unsafe fn bor(a: Term, b: Term) -> Term {
    if is_small(a) && is_small(b) {
        return small(small_val(a) | small_val(b));
    }
    int_op(a, b, |x, y| Some(x | y))
}
pub unsafe fn bxor(a: Term, b: Term) -> Term {
    if is_small(a) && is_small(b) {
        return small(small_val(a) ^ small_val(b));
    }
    int_op(a, b, |x, y| Some(x ^ y))
}
pub unsafe fn bnot(a: Term) -> Term {
    if is_small(a) {
        return small(!small_val(a));
    }
    match to_big(a) {
        Some(x) => build::big(&(-x - 1)),
        None => exc::error(crate::atoms::BADARITH),
    }
}

pub unsafe fn bsl(a: Term, b: Term) -> Term {
    shift(a, b, false)
}
pub unsafe fn bsr(a: Term, b: Term) -> Term {
    shift(a, b, true)
}

unsafe fn shift(a: Term, b: Term, right: bool) -> Term {
    let (x, n) = match (to_big(a), to_big(b)) {
        (Some(x), Some(n)) => (x, n),
        _ => return exc::error(crate::atoms::BADARITH),
    };
    let n = match n.to_i64() {
        Some(n) => n,
        None => return exc::error(crate::atoms::SYSTEM_LIMIT),
    };
    let n = if right { -n } else { n };
    if n >= 0 {
        if n > (1 << 24) {
            return exc::error(crate::atoms::SYSTEM_LIMIT);
        }
        build::big(&(x << n as usize))
    } else {
        let m = (-n) as u64;
        if m > (1 << 24) {
            return build::int(if x.is_negative() { -1 } else { 0 });
        }
        // Arithmetic shift right (floor), like Erlang.
        build::big(&x.div_floor(&(BigInt::from(1) << m as usize)))
    }
}

pub unsafe fn neg(a: Term) -> Term {
    if is_small(a) {
        return build::int(-small_val(a));
    }
    match to_num(a) {
        Some(Num::I(x)) => build::big(&-x),
        Some(Num::F(f)) => build::float(-f),
        None => exc::error(crate::atoms::BADARITH),
    }
}

pub unsafe fn abs(a: Term) -> Term {
    match to_num(a) {
        Some(Num::I(x)) => build::big(&x.abs()),
        Some(Num::F(f)) => build::float(f.abs()),
        None => exc::error(crate::atoms::BADARG),
    }
}

pub unsafe fn float_of(a: Term) -> Term {
    match to_f64(a) {
        Some(f) if is_number(a) => build::float(f),
        _ => exc::error(crate::atoms::BADARG),
    }
}

/// trunc/round/floor/ceil of a number.
pub unsafe fn to_int(a: Term, how: fn(f64) -> f64) -> Term {
    if is_integer(a) {
        return a;
    }
    if is_float(a) {
        let f = how(float_val(a));
        if f.abs() < 9.0e15 {
            return build::int(f as i64);
        }
        return build::big(&f64_to_big(f));
    }
    exc::error(crate::atoms::BADARG)
}

pub fn f64_to_big(f: f64) -> BigInt {
    use num_traits::FromPrimitive;
    BigInt::from_f64(f).unwrap_or_default()
}

pub fn round_half_away(f: f64) -> f64 {
    f.round()
}

pub unsafe fn to_i64(t: Term) -> Option<i64> {
    if is_small(t) {
        Some(small_val(t))
    } else {
        to_big(t).and_then(|b| b.to_i64())
    }
}
