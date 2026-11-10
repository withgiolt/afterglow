//! NIFs of the `binary` and `unicode` modules.

use crate::atoms::{self, *};
use crate::bits::{self, Writer};
use crate::build;
use crate::exc::badarg;
use crate::term::*;
use num_bigint::{BigInt, Sign};

macro_rules! bif {
    ($name:literal, fn $f:ident($($a:ident),*) $body:block) => {
        #[export_name = $name]
        pub unsafe extern "C" fn $f($($a: Term),*) -> Term $body
    };
}

unsafe fn bytes_of(t: Term) -> Option<&'static [u8]> {
    if !is_binary(t) {
        return None;
    }
    let b = bin_of(t);
    if b.bitoff != 0 {
        // Unaligned binaries: copy into the arena.
        let v = b.to_vec();
        let p = crate::mem::balloc(v.len());
        core::ptr::copy_nonoverlapping(v.as_ptr(), p, v.len());
        return Some(core::slice::from_raw_parts(p, v.len()));
    }
    Some(b.slice())
}

/// Sub-binary of `t` (byte aligned) from `pos`, `len` bytes, sharing the bytes.
unsafe fn sub(t: Term, pos: usize, len: usize) -> Term {
    let b = bin_of(t);
    let abs = b.bitoff + 8 * pos as u64;
    build::bin_ref(b.base.add((abs / 8) as usize), abs % 8, 8 * len as u64)
}

// ---------------------------------------------------------------------------
// Patterns

/// A compiled pattern is {afterglow_cp, [Bin...]}; raw patterns are a binary or a list of
/// binaries. None if invalid (empty patterns are invalid).
unsafe fn patterns(p: Term) -> Option<Vec<&'static [u8]>> {
    let mut p = p;
    if is_tuple(p) && tuple_arity(p) == 2 && tuple_get(p, 0) == atoms::intern("afterglow_cp") {
        p = tuple_get(p, 1);
    }
    let items = if is_binary(p) { vec![p] } else { build::list_to_vec(p)? };
    if items.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for it in items {
        let b = bytes_of(it)?;
        if b.is_empty() {
            return None;
        }
        out.push(b);
    }
    Some(out)
}

/// First match in hay[from..to]: leftmost, then longest. (pos, len)
fn find(hay: &[u8], pats: &[&[u8]], from: usize, to: usize) -> Option<(usize, usize)> {
    let first_bytes: Vec<u8> = pats.iter().map(|p| p[0]).collect();
    let mut i = from;
    while i < to {
        let c = hay[i];
        if first_bytes.contains(&c) {
            let mut best: Option<usize> = None;
            for p in pats {
                if i + p.len() <= to && &hay[i..i + p.len()] == *p {
                    best = Some(best.map_or(p.len(), |b: usize| b.max(p.len())));
                }
            }
            if let Some(l) = best {
                return Some((i, l));
            }
        }
        i += 1;
    }
    None
}

struct Opts {
    from: usize,
    to: usize,
    global: bool,
    trim: bool,
    trim_all: bool,
}

unsafe fn opts(o: Term, size: usize) -> Option<Opts> {
    let mut r = Opts { from: 0, to: size, global: false, trim: false, trim_all: false };
    for it in build::list_to_vec(o)? {
        if is_atom(it) {
            match atoms::name(it) {
                "global" => r.global = true,
                "trim" => r.trim = true,
                "trim_all" => r.trim_all = true,
                _ => return None,
            }
        } else if is_tuple(it) && tuple_arity(it) == 2 && tuple_get(it, 0) == SCOPE {
            let sc = tuple_get(it, 1);
            if !is_tuple(sc) || tuple_arity(sc) != 2 {
                return None;
            }
            let (s, l) = (tuple_get(sc, 0), tuple_get(sc, 1));
            if !is_small(s) || !is_small(l) {
                return None;
            }
            let (mut s, mut l) = (small_val(s), small_val(l));
            if l < 0 {
                s += l;
                l = -l;
            }
            if s < 0 || (s + l) as usize > size {
                return None;
            }
            r.from = s as usize;
            r.to = (s + l) as usize;
        } else {
            return None;
        }
    }
    Some(r)
}

bif!("binary:compile_pattern/1", fn compile_pattern(p) {
    if patterns(p).is_none() { return badarg(); }
    let l = if is_binary(p) { build::list(&[p]) } else { p };
    build::tuple(&[atoms::intern("afterglow_cp"), l])
});

unsafe fn match_impl(s: Term, p: Term, o: Term) -> Term {
    let (Some(hay), Some(pats)) = (bytes_of(s), patterns(p)) else { return badarg() };
    let Some(op) = opts(o, hay.len()) else { return badarg() };
    match find(hay, &pats, op.from, op.to) {
        Some((pos, len)) => build::tuple(&[small(pos as i64), small(len as i64)]),
        None => NOMATCH,
    }
}
bif!("binary:match/2", fn bmatch2(s, p) { match_impl(s, p, NIL) });
bif!("binary:match/3", fn bmatch3(s, p, o) { match_impl(s, p, o) });

unsafe fn matches_impl(s: Term, p: Term, o: Term) -> Term {
    let (Some(hay), Some(pats)) = (bytes_of(s), patterns(p)) else { return badarg() };
    let Some(op) = opts(o, hay.len()) else { return badarg() };
    let mut out = Vec::new();
    let mut i = op.from;
    while let Some((pos, len)) = find(hay, &pats, i, op.to) {
        out.push(build::tuple(&[small(pos as i64), small(len as i64)]));
        i = pos + len;
    }
    build::list(&out)
}
bif!("binary:matches/2", fn bmatches2(s, p) { matches_impl(s, p, NIL) });
bif!("binary:matches/3", fn bmatches3(s, p, o) { matches_impl(s, p, o) });

unsafe fn split_impl(s: Term, p: Term, o: Term) -> Term {
    let (Some(hay), Some(pats)) = (bytes_of(s), patterns(p)) else { return badarg() };
    let Some(op) = opts(o, hay.len()) else { return badarg() };
    let mut parts: Vec<Term> = Vec::new();
    let mut start = 0usize;
    let mut i = op.from;
    while let Some((pos, len)) = find(hay, &pats, i, op.to) {
        parts.push(sub(s, start, pos - start));
        start = pos + len;
        i = start;
        if !op.global {
            break;
        }
    }
    parts.push(sub(s, start, hay.len() - start));
    if op.trim_all {
        parts.retain(|&t| bin_of(t).bits != 0);
    } else if op.trim {
        while parts.len() > 0 && bin_of(*parts.last().unwrap()).bits == 0 {
            parts.pop();
        }
    }
    build::list(&parts)
}
bif!("binary:split/2", fn bsplit2(s, p) { split_impl(s, p, NIL) });
bif!("binary:split/3", fn bsplit3(s, p, o) { split_impl(s, p, o) });

bif!("binary:at/2", fn bat(b, i) {
    let Some(v) = bytes_of(b) else { return badarg() };
    if !is_small(i) || small_val(i) < 0 || small_val(i) as usize >= v.len() { return badarg(); }
    small(v[small_val(i) as usize] as i64)
});
bif!("binary:first/1", fn bfirst(b) {
    match bytes_of(b) { Some(v) if !v.is_empty() => small(v[0] as i64), _ => badarg() }
});
bif!("binary:last/1", fn blast(b) {
    match bytes_of(b) { Some(v) if !v.is_empty() => small(v[v.len() - 1] as i64), _ => badarg() }
});
bif!("binary:copy/1", fn bcopy1(b) {
    match bytes_of(b) { Some(v) => build::binary(v), None => badarg() }
});
bif!("binary:copy/2", fn bcopy2(b, n) {
    let Some(v) = bytes_of(b) else { return badarg() };
    if !is_small(n) || small_val(n) < 0 { return badarg(); }
    let v = v.to_vec();
    build::binary(&v.repeat(small_val(n) as usize))
});
bif!("binary:referenced_byte_size/1", fn bref_size(b) {
    match bytes_of(b) { Some(v) => small(v.len() as i64), None => badarg() }
});
bif!("binary:list_to_bin/1", fn blist_to_bin(l) {
    let mut w = Writer::new();
    if !bits::iolist(l, &mut w, false) { return badarg(); }
    w.finish()
});
bif!("binary:longest_common_prefix/1", fn blcp(l) {
    let Some(items) = build::list_to_vec(l) else { return badarg() };
    let mut bs = Vec::new();
    for it in items { match bytes_of(it) { Some(b) => bs.push(b), None => return badarg() } }
    if bs.is_empty() { return badarg(); }
    let mut n = 0;
    'outer: loop {
        for b in &bs { if n >= b.len() || b[n] != bs[0][n] { break 'outer; } }
        n += 1;
    }
    small(n as i64)
});
bif!("binary:longest_common_suffix/1", fn blcs(l) {
    let Some(items) = build::list_to_vec(l) else { return badarg() };
    let mut bs = Vec::new();
    for it in items { match bytes_of(it) { Some(b) => bs.push(b), None => return badarg() } }
    if bs.is_empty() { return badarg(); }
    let mut n = 0;
    'outer: loop {
        for b in &bs { if n >= b.len() || b[b.len() - 1 - n] != bs[0][bs[0].len() - 1 - n] { break 'outer; } }
        n += 1;
    }
    small(n as i64)
});
unsafe fn little(e: Term) -> Option<bool> {
    match atoms::name(e) {
        "big" => Some(false),
        "little" => Some(true),
        _ => None,
    }
}
bif!("binary:encode_unsigned/1", fn benc1(i) { benc2(i, BIG) });
bif!("binary:encode_unsigned/2", fn benc2(i, e) {
    let Some(l) = (if is_atom(e) { little(e) } else { None }) else { return badarg() };
    let v = match crate::num::to_big(i) { Some(v) if v.sign() != Sign::Minus => v, _ => return badarg() };
    let mut bytes = if l { v.to_bytes_le().1 } else { v.to_bytes_be().1 };
    if bytes.is_empty() { bytes.push(0); }
    build::binary(&bytes)
});
bif!("binary:decode_unsigned/1", fn bdec1(b) { bdec2(b, BIG) });
bif!("binary:decode_unsigned/2", fn bdec2(b, e) {
    let Some(l) = (if is_atom(e) { little(e) } else { None }) else { return badarg() };
    let Some(v) = bytes_of(b) else { return badarg() };
    let n = if l { BigInt::from_bytes_le(Sign::Plus, v) } else { BigInt::from_bytes_be(Sign::Plus, v) };
    build::big(&n)
});

// ---------------------------------------------------------------------------
// unicode

#[derive(Clone, Copy, PartialEq)]
enum Enc {
    Latin1,
    Utf8,
}

unsafe fn enc_of(t: Term) -> Option<Enc> {
    if !is_atom(t) {
        return None;
    }
    match atoms::name(t) {
        "latin1" => Some(Enc::Latin1),
        "unicode" | "utf8" => Some(Enc::Utf8),
        _ => None,
    }
}

enum Stop {
    Error,
    Incomplete,
}

/// Walks chardata, pushing code points. On failure returns how it stopped and what
/// remains (the failing element, followed by the rest of the input).
unsafe fn walk(t: Term, enc: Enc, out: &mut Vec<u32>) -> Result<(), (Stop, Term)> {
    // Explicit stack of pending tails.
    let mut stack: Vec<Term> = vec![t];
    while let Some(t) = stack.pop() {
        let rest_of = |stack: &Vec<Term>, cur: Term| -> Term {
            // the failing element followed by everything pending
            let mut r = cur;
            for &pending in stack.iter() {
                if pending != NIL {
                    r = build::list(&[r, pending]);
                }
            }
            r
        };
        if t == NIL {
            continue;
        }
        if is_binary(t) {
            let b = bytes_of(t).unwrap();
            match enc {
                Enc::Latin1 => out.extend(b.iter().map(|&c| c as u32)),
                Enc::Utf8 => {
                    let mut i = 0;
                    while i < b.len() {
                        match core::str::from_utf8(&b[i..]) {
                            Ok(s) => {
                                out.extend(s.chars().map(|c| c as u32));
                                i = b.len();
                            }
                            Err(e) => {
                                let good = e.valid_up_to();
                                out.extend(core::str::from_utf8_unchecked(&b[i..i + good]).chars().map(|c| c as u32));
                                let at = i + good;
                                let rest = sub(t, at, b.len() - at);
                                let stop = if e.error_len().is_none() { Stop::Incomplete } else { Stop::Error };
                                return Err((stop, rest_of(&stack, rest)));
                            }
                        }
                    }
                }
            }
        } else if is_list(t) {
            let h = head(t);
            let tl = tail(t);
            if is_small(h) {
                let c = small_val(h);
                let ok = c >= 0 && c <= 0x10FFFF && !(0xD800..=0xDFFF).contains(&c) && (enc == Enc::Utf8 || c <= 255);
                if !ok {
                    let r = rest_of(&stack, t);
                    return Err((Stop::Error, r));
                }
                out.push(c as u32);
                stack.push(tl);
            } else {
                stack.push(tl);
                stack.push(h);
            }
        } else {
            return Err((Stop::Error, NON_VALUE)); // not chardata: badarg
        }
    }
    Ok(())
}

fn utf8_bytes(cps: &[u32]) -> Vec<u8> {
    let mut s = String::with_capacity(cps.len());
    for &c in cps {
        s.push(char::from_u32(c).unwrap());
    }
    s.into_bytes()
}

bif!("unicode:characters_to_binary/2", fn c2b(data, inenc) {
    let Some(enc) = enc_of(inenc) else { return badarg() };
    // Fast path: a valid UTF-8 binary converts to itself.
    if enc == Enc::Utf8 && is_binary(data) {
        if let Some(b) = bytes_of(data) {
            if core::str::from_utf8(b).is_ok() { return data; }
        }
    }
    let mut cps = Vec::new();
    match walk(data, enc, &mut cps) {
        Ok(()) => build::binary(&utf8_bytes(&cps)),
        Err((_, NON_VALUE)) => badarg(),
        Err((stop, rest)) => {
            let done = build::binary(&utf8_bytes(&cps));
            let tag = match stop { Stop::Error => ERROR, Stop::Incomplete => INCOMPLETE };
            build::tuple(&[tag, done, rest])
        }
    }
});

bif!("unicode:characters_to_list/2", fn c2l(data, inenc) {
    let Some(enc) = enc_of(inenc) else { return badarg() };
    let mut cps = Vec::new();
    let mk = |cps: &Vec<u32>| {
        let v: Vec<Term> = cps.iter().map(|&c| small(c as i64)).collect();
        build::list(&v)
    };
    match walk(data, enc, &mut cps) {
        Ok(()) => mk(&cps),
        Err((_, NON_VALUE)) => badarg(),
        Err((stop, rest)) => {
            let done = mk(&cps);
            let tag = match stop { Stop::Error => ERROR, Stop::Incomplete => INCOMPLETE };
            build::tuple(&[tag, done, rest])
        }
    }
});
