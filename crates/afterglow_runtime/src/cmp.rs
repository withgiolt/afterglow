//! Term order and equality.
//! number < atom < reference < fun < port < pid < tuple < map < nil < list < bitstring

use crate::atoms;
use crate::build;
use crate::num;
use crate::term::*;
use core::cmp::Ordering;

fn class(t: Term) -> u8 {
    if is_small(t) {
        return 0;
    }
    if is_atom(t) {
        return 1;
    }
    if t == NIL {
        return 8;
    }
    if is_list(t) {
        return 9;
    }
    if is_pid(t) {
        return 5;
    }
    if is_port(t) {
        return 4;
    }
    match boxed_sub(t) {
        Some(SUB_POS_BIG) | Some(SUB_NEG_BIG) | Some(SUB_FLOAT) => 0,
        Some(SUB_REF) => 2,
        Some(SUB_FUN) => 3,
        Some(SUB_TUPLE) => 6,
        Some(SUB_MAP) => 7,
        Some(SUB_BIN) | Some(SUB_MATCH) => 10,
        _ => 11,
    }
}

/// Erlang term order (`<`, `==`): integers and floats compare by value.
pub fn compare(a: Term, b: Term) -> Ordering {
    unsafe { cmp(a, b, false) }
}

/// Order used for map keys and exact equality (`=:=`): every integer comes before
/// every float.
pub fn map_key_cmp(a: Term, b: Term) -> Ordering {
    unsafe { cmp(a, b, true) }
}

unsafe fn cmp(a: Term, b: Term, exact: bool) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }
    let (ca, cb) = (class(a), class(b));
    if ca != cb {
        return ca.cmp(&cb);
    }
    match ca {
        0 => {
            if is_small(a) && is_small(b) {
                return small_val(a).cmp(&small_val(b));
            }
            let fa = is_float(a);
            let fb = is_float(b);
            if fa && fb {
                return float_val(a).partial_cmp(&float_val(b)).unwrap_or(Ordering::Equal);
            }
            if !fa && !fb {
                return num::to_big(a).unwrap().cmp(&num::to_big(b).unwrap());
            }
            // Mixed integer and float.
            if exact {
                return if fa { Ordering::Greater } else { Ordering::Less };
            }
            if fa {
                cmp_float_int(float_val(a), b)
            } else {
                cmp_float_int(float_val(b), a).reverse()
            }
        }
        1 => atoms::name(a).as_bytes().cmp(atoms::name(b).as_bytes()),
        2 => (*ptr(a).add(1)).cmp(&*ptr(b).add(1)),
        3 => {
            // funs: by module, then name/index, then free variables
            for i in [3usize, 4] {
                let o = cmp(*ptr(a).add(i), *ptr(b).add(i), exact);
                if o != Ordering::Equal {
                    return o;
                }
            }
            let (na, nb) = (hdr_arity(*ptr(a)), hdr_arity(*ptr(b)));
            if na != nb {
                return na.cmp(&nb);
            }
            for i in 5..=na as usize {
                let o = cmp(*ptr(a).add(i), *ptr(b).add(i), exact);
                if o != Ordering::Equal {
                    return o;
                }
            }
            Ordering::Equal
        }
        4 | 5 => a.cmp(&b),
        6 => {
            let (na, nb) = (tuple_arity(a), tuple_arity(b));
            if na != nb {
                return na.cmp(&nb);
            }
            for i in 0..na {
                let o = cmp(tuple_get(a, i), tuple_get(b, i), exact);
                if o != Ordering::Equal {
                    return o;
                }
            }
            Ordering::Equal
        }
        7 => {
            let (na, nb) = (build::map_size(a), build::map_size(b));
            if na != nb {
                return na.cmp(&nb);
            }
            for i in 0..na {
                let o = cmp(build::map_key(a, i), build::map_key(b, i), true);
                if o != Ordering::Equal {
                    return o;
                }
            }
            for i in 0..na {
                let o = cmp(build::map_val(a, i), build::map_val(b, i), exact);
                if o != Ordering::Equal {
                    return o;
                }
            }
            Ordering::Equal
        }
        8 => Ordering::Equal,
        9 => {
            let (mut x, mut y) = (a, b);
            loop {
                if x == y {
                    return Ordering::Equal;
                }
                if !is_list(x) || !is_list(y) {
                    return cmp(x, y, exact);
                }
                let o = cmp(head(x), head(y), exact);
                if o != Ordering::Equal {
                    return o;
                }
                x = tail(x);
                y = tail(y);
            }
        }
        10 => {
            let (ba, bb) = (bin_of(a), bin_of(b));
            if ba.is_byte_aligned() && bb.is_byte_aligned() {
                return ba.slice().cmp(bb.slice());
            }
            let n = ba.bits.min(bb.bits);
            for i in 0..n {
                let o = ba.bit(i).cmp(&bb.bit(i));
                if o != Ordering::Equal {
                    return o;
                }
            }
            ba.bits.cmp(&bb.bits)
        }
        _ => a.cmp(&b),
    }
}

unsafe fn cmp_float_int(f: f64, i: Term) -> Ordering {
    if is_small(i) {
        let v = small_val(i);
        // Exact for |v| < 2^53; otherwise compare as big.
        if v.abs() < (1 << 53) {
            return f.partial_cmp(&(v as f64)).unwrap_or(Ordering::Equal);
        }
    }
    let b = num::to_big(i).unwrap();
    if f.is_nan() {
        return Ordering::Equal;
    }
    let ff = f.floor();
    let fb = num::f64_to_big(ff);
    match fb.cmp(&b) {
        Ordering::Equal => {
            if f > ff {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }
        o => o,
    }
}

pub fn eq(a: Term, b: Term) -> bool {
    compare(a, b) == Ordering::Equal
}

pub fn exact_eq(a: Term, b: Term) -> bool {
    if a == b {
        return true;
    }
    if is_immed(a) || is_immed(b) {
        return false;
    }
    map_key_cmp(a, b) == Ordering::Equal
}

#[no_mangle]
pub extern "C" fn rt_exact_eq(a: Term, b: Term) -> i32 {
    exact_eq(a, b) as i32
}
#[no_mangle]
pub extern "C" fn rt_eq(a: Term, b: Term) -> i32 {
    eq(a, b) as i32
}
/// -1, 0, 1
#[no_mangle]
pub extern "C" fn rt_cmp(a: Term, b: Term) -> i32 {
    match compare(a, b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}
