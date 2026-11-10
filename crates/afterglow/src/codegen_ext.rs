//! Binary and map instructions. Most of the work is done by afterglow_runtime (bits.rs, maps.rs);
//! this file passes operands. Multi-operand instructions (bs_create_bin, put_map_*)
//! write their operands into a scratch area in XREGS (words 512..1023), which is not
//! a GC root and is consumed before anything can collect.

use crate::codegen::{CgError, Ext};
use beam_reader::{Instr, Operand};
use wasm_encoder::{BlockType, Instruction as I, MemArg, ValType};

const SCRATCH: u64 = 512 * 8;
const F_LITTLE: i32 = 1;
const F_SIGNED: i32 = 2;

fn mem64(offset: u64) -> MemArg {
    MemArg { offset, align: 3, memory_index: 0 }
}

fn label(o: &Operand) -> u32 {
    match o {
        Operand::Label(l) => *l,
        _ => 0,
    }
}

fn uval(o: &Operand) -> Result<u64, CgError> {
    match o.plain() {
        Operand::U(v) => Ok(*v),
        Operand::I(v) if *v >= 0 => Ok(*v as u64),
        other => Err(CgError(format!("expected an unsigned literal, got {:?}", other))),
    }
}

/// Field flags as afterglow_runtime's bits (1 little, 2 signed).
fn flags(x: &Ext, o: &Operand) -> i32 {
    let from_names = |names: &[&str]| {
        let mut f = 0;
        for n in names {
            match *n {
                "little" | "native" => f |= F_LITTLE,
                "signed" => f |= F_SIGNED,
                _ => {}
            }
        }
        f
    };
    match o.plain() {
        Operand::U(n) => {
            let mut f = 0;
            if n & 0x02 != 0 || n & 0x10 != 0 {
                f |= F_LITTLE;
            }
            if n & 0x04 != 0 {
                f |= F_SIGNED;
            }
            f
        }
        Operand::List(items) => {
            let names: Vec<&str> = items
                .iter()
                .filter_map(|i| if let Operand::Atom(a) = i { Some(x.atom_name(*a)) } else { None })
                .collect();
            from_names(&names)
        }
        Operand::Literal(i) => {
            let etf = x.module_literal(*i);
            let s = String::from_utf8_lossy(etf);
            let mut names = Vec::new();
            for n in ["little", "native", "signed"] {
                if s.contains(n) {
                    names.push(n);
                }
            }
            from_names(&names)
        }
        _ => 0,
    }
}

fn binary_etf(bytes: &[u8]) -> Vec<u8> {
    let mut v = vec![131, 109];
    v.extend((bytes.len() as u32).to_be_bytes());
    v.extend_from_slice(bytes);
    v
}

/// Stores the value produced by `val` into scratch word `w`.
fn scratch(x: &mut Ext, w: u64, val: impl FnOnce(&mut Ext) -> Result<(), CgError>) -> Result<(), CgError> {
    x.ins(I::GlobalGet(x.g_xr()));
    val(x)?;
    x.ins(I::I64Store(mem64(SCRATCH + 8 * w)));
    Ok(())
}

fn scratch_addr(x: &mut Ext) {
    x.ins(I::GlobalGet(x.g_xr()));
    x.ins(I::I32Const(SCRATCH as i32));
    x.ins(I::I32Add);
}

/// Calls a runtime function returning a term; 0 jumps to `fail` (or raises when
/// fail is 0 and the runtime set an exception); otherwise stores into `dst`.
fn call_term(x: &mut Ext, fail: u32, dst: &Operand, push: impl FnOnce(&mut Ext) -> Result<(), CgError>, rt: &str) -> Result<(), CgError> {
    push(x)?;
    let f = x.rt(rt);
    x.ins(I::Call(f));
    x.check_or_fail(fail)?;
    let t = x.tmp(2);
    x.ins(I::LocalSet(t));
    x.dst(dst, |x| {
        x.ins(I::LocalGet(t));
        Ok(())
    })
}

/// Calls a runtime test returning i32; 0 jumps to `fail`.
fn call_test(x: &mut Ext, fail: u32, push: impl FnOnce(&mut Ext) -> Result<(), CgError>, rt: &str) -> Result<(), CgError> {
    push(x)?;
    let f = x.rt(rt);
    x.ins(I::Call(f));
    x.test_jump(fail)
}


// ---------------------------------------------------------------------------
// Inline binary matching. Match context: [hdr, bin term, small(pos)];
// binary: [hdr, small(bits), small(byte base), small(bit offset)].
// Locals: bs(0) = pos, bs(1) = end (bit size), bs(2) = bit address of bit 0;
// tmpi(1) = context address.

fn m64(off: u64) -> MemArg {
    MemArg { offset: off, align: 3, memory_index: 0 }
}
fn m8(off: u64) -> MemArg {
    MemArg { offset: off, align: 0, memory_index: 0 }
}

fn bs_prologue(x: &mut Ext, ctx: &Operand) -> Result<(), CgError> {
    let (p, pos, end, base) = (x.tmpi(1), x.bs(0), x.bs(1), x.bs(2));
    x.src(ctx)?;
    x.ins(I::I32WrapI64);
    x.ins(I::I32Const(2));
    x.ins(I::I32Sub);
    x.ins(I::LocalTee(p));
    x.ins(I::I64Load(m64(16)));
    x.ins(I::I64Const(4));
    x.ins(I::I64ShrS);
    x.ins(I::LocalSet(pos));
    // binary object address (term - 2), fields at +8, +16, +24
    let b = x.tmpi(2);
    x.ins(I::LocalGet(p));
    x.ins(I::I64Load(m64(8)));
    x.ins(I::I32WrapI64);
    x.ins(I::I32Const(2));
    x.ins(I::I32Sub);
    x.ins(I::LocalTee(b));
    x.ins(I::I64Load(m64(8)));
    x.ins(I::I64Const(4));
    x.ins(I::I64ShrS);
    x.ins(I::LocalSet(end));
    x.ins(I::LocalGet(b));
    x.ins(I::I64Load(m64(16)));
    x.ins(I::I64Const(4));
    x.ins(I::I64ShrS);
    x.ins(I::I64Const(3));
    x.ins(I::I64Shl);
    x.ins(I::LocalGet(b));
    x.ins(I::I64Load(m64(24)));
    x.ins(I::I64Const(4));
    x.ins(I::I64ShrS);
    x.ins(I::I64Add);
    x.ins(I::LocalSet(base));
    Ok(())
}

/// Stores pos back into the context.
fn bs_sync(x: &mut Ext) {
    let (p, pos) = (x.tmpi(1), x.bs(0));
    x.ins(I::LocalGet(p));
    x.ins(I::LocalGet(pos));
    x.ins(I::I64Const(4));
    x.ins(I::I64Shl);
    x.ins(I::I64Const(0xF));
    x.ins(I::I64Or);
    x.ins(I::I64Store(m64(16)));
}

fn bs_reload(x: &mut Ext) {
    let (p, pos) = (x.tmpi(1), x.bs(0));
    x.ins(I::LocalGet(p));
    x.ins(I::I64Load(m64(16)));
    x.ins(I::I64Const(4));
    x.ins(I::I64ShrS);
    x.ins(I::LocalSet(pos));
}

fn bs_ctx_term(x: &mut Ext) {
    let p = x.tmpi(1);
    x.ins(I::LocalGet(p));
    x.ins(I::I64ExtendI32U);
    x.ins(I::I64Const(2));
    x.ins(I::I64Add);
}

fn bs_advance(x: &mut Ext, n: u64) {
    let pos = x.bs(0);
    x.ins(I::LocalGet(pos));
    x.ins(I::I64Const(n as i64));
    x.ins(I::I64Add);
    x.ins(I::LocalSet(pos));
    bs_sync(x);
}

/// Pushes (end - pos) as i64.
fn bs_remaining(x: &mut Ext) {
    let (pos, end) = (x.bs(0), x.bs(1));
    x.ins(I::LocalGet(end));
    x.ins(I::LocalGet(pos));
    x.ins(I::I64Sub);
}

/// Pushes the next n (1..=8) bits as an unsigned i64, without advancing.
/// Reads two bytes; the second may lie past the binary but is inside linear memory
/// and masked off.
fn bs_peek_small(x: &mut Ext, n: u64) {
    let (pos, base) = (x.bs(0), x.bs(2));
    let a = x.tmpi(3);
    x.ins(I::LocalGet(base));
    x.ins(I::LocalGet(pos));
    x.ins(I::I64Add);
    x.ins(I::I32WrapI64);
    x.ins(I::LocalTee(a));
    x.ins(I::I32Const(3));
    x.ins(I::I32ShrU);
    x.ins(I::I32Load8U(m8(0)));
    x.ins(I::I32Const(8));
    x.ins(I::I32Shl);
    x.ins(I::LocalGet(a));
    x.ins(I::I32Const(3));
    x.ins(I::I32ShrU);
    x.ins(I::I32Load8U(m8(1)));
    x.ins(I::I32Or);
    // >> (16 - (a & 7) - n)
    x.ins(I::I32Const(16 - n as i32));
    x.ins(I::LocalGet(a));
    x.ins(I::I32Const(7));
    x.ins(I::I32And);
    x.ins(I::I32Sub);
    x.ins(I::I32ShrU);
    x.ins(I::I32Const(((1u32 << n) - 1) as i32));
    x.ins(I::I32And);
    x.ins(I::I64ExtendI32U);
}

fn small_const(o: &Operand) -> Option<i64> {
    match o.plain() {
        Operand::I(v) => Some(*v),
        Operand::U(v) => Some(*v as i64),
        _ => None,
    }
}

/// bs_match with inline fast paths; slow paths call afterglow_runtime with pos synced.
fn bs_match(x: &mut Ext, fail: u32, ctx: &Operand, cmds: &[Operand]) -> Result<(), CgError> {
    bs_prologue(x, ctx)?;
    let mut i = 0;
    while i < cmds.len() {
        let Operand::Atom(name) = &cmds[i] else { return Err(CgError(format!("bad bs_match command {:?}", cmds[i]))) };
        let name = x.atom_name(*name).to_string();
        match name.as_str() {
            "ensure_at_least" => {
                let (size, unit) = (uval(&cmds[i + 1])?, uval(&cmds[i + 2])?);
                bs_remaining(x);
                x.ins(I::I64Const(size as i64));
                x.ins(I::I64GeS);
                x.test_jump(fail)?;
                if unit > 1 {
                    bs_remaining(x);
                    x.ins(I::I64Const(size as i64));
                    x.ins(I::I64Sub);
                    x.ins(I::I64Const(unit as i64));
                    x.ins(I::I64RemU);
                    x.ins(I::I64Eqz);
                    x.test_jump(fail)?;
                }
                i += 3;
            }
            "ensure_exactly" => {
                let size = uval(&cmds[i + 1])?;
                bs_remaining(x);
                x.ins(I::I64Const(size as i64));
                x.ins(I::I64Eq);
                x.test_jump(fail)?;
                i += 2;
            }
            "=:=" => {
                let size = uval(&cmds[i + 2])?;
                let value = cmds[i + 3].clone();
                match small_const(&value) {
                    Some(v) if (1..=8).contains(&size) && v >= 0 => {
                        bs_peek_small(x, size);
                        x.ins(I::I64Const(v));
                        x.ins(I::I64Eq);
                        x.test_jump(fail)?;
                        bs_advance(x, size);
                    }
                    _ => {
                        call_test(x, fail, |x| {
                            bs_ctx_term(x);
                            x.ins(I::I64Const(size as i64));
                            x.src(&value)
                        }, "rt_bs_eq")?;
                        bs_reload(x);
                    }
                }
                i += 4;
            }
            "integer" => {
                let fl = flags(x, &cmds[i + 2]);
                let bits = uval(&cmds[i + 3])? * uval(&cmds[i + 4])?;
                let dst = cmds[i + 5].clone();
                if (1..=8).contains(&bits) && fl == 0 {
                    x.dst(&dst, |x| {
                        bs_peek_small(x, bits);
                        x.ins(I::I64Const(4));
                        x.ins(I::I64Shl);
                        x.ins(I::I64Const(0xF));
                        x.ins(I::I64Or);
                        Ok(())
                    })?;
                    bs_advance(x, bits);
                } else {
                    x.dst(&dst, |x| {
                        bs_ctx_term(x);
                        x.ins(I::I64Const(bits as i64));
                        x.ins(I::I32Const(fl));
                        let f = x.rt("rt_bs_int");
                        x.ins(I::Call(f));
                        Ok(())
                    })?;
                    bs_reload(x);
                }
                i += 6;
            }
            "binary" => {
                let bits = uval(&cmds[i + 3])? * uval(&cmds[i + 4])?;
                let dst = cmds[i + 5].clone();
                x.dst(&dst, |x| {
                    bs_ctx_term(x);
                    x.ins(I::I64Const(bits as i64));
                    let f = x.rt("rt_bs_bin");
                    x.ins(I::Call(f));
                    Ok(())
                })?;
                bs_reload(x);
                i += 6;
            }
            "get_tail" => {
                let dst = cmds[i + 3].clone();
                x.dst(&dst, |x| {
                    bs_ctx_term(x);
                    let f = x.rt("rt_bs_tail");
                    x.ins(I::Call(f));
                    Ok(())
                })?;
                bs_reload(x);
                i += 4;
            }
            "skip" => {
                let bits = uval(&cmds[i + 1])?;
                bs_advance(x, bits);
                i += 2;
            }
            other => return Err(CgError(format!("unsupported bs_match command {}", other))),
        }
    }
    Ok(())
}

/// bs_start_match3/4: a context passes through, a binary gets a new context inline.
fn bs_start(x: &mut Ext, fail: u32, src: &Operand, dst: &Operand) -> Result<(), CgError> {
    let (t, h) = (x.tmp(0), x.tmp(1));
    x.src(src)?;
    x.ins(I::LocalTee(t));
    x.ins(I::I64Const(3));
    x.ins(I::I64And);
    x.ins(I::I64Const(2));
    x.ins(I::I64Eq);
    x.ins(I::If(BlockType::Result(ValType::I64)));
    {
        // sub tag of the header
        x.ins(I::LocalGet(t));
        x.ins(I::I32WrapI64);
        x.ins(I::I32Const(2));
        x.ins(I::I32Sub);
        x.ins(I::I64Load(m64(0)));
        x.ins(I::I64Const(2));
        x.ins(I::I64ShrU);
        x.ins(I::I64Const(0xF));
        x.ins(I::I64And);
        x.ins(I::LocalTee(h));
        x.ins(I::I64Const(9));
        x.ins(I::I64Eq);
        x.ins(I::If(BlockType::Result(ValType::I64)));
        x.ins(I::LocalGet(t));
        x.ins(I::Else);
        x.ins(I::LocalGet(h));
        x.ins(I::I64Const(7));
        x.ins(I::I64Eq);
        x.ins(I::If(BlockType::Result(ValType::I64)));
        let p = x.alloc_words(3);
        x.ins(I::LocalGet(p));
        x.ins(I::I64Const((2 << 6) | (9 << 2)));
        x.ins(I::I64Store(m64(0)));
        x.ins(I::LocalGet(p));
        x.ins(I::LocalGet(t));
        x.ins(I::I64Store(m64(8)));
        x.ins(I::LocalGet(p));
        x.ins(I::I64Const(0xF));
        x.ins(I::I64Store(m64(16)));
        x.ins(I::LocalGet(p));
        x.ins(I::I64ExtendI32U);
        x.ins(I::I64Const(2));
        x.ins(I::I64Or);
        x.ins(I::Else);
        x.ins(I::LocalGet(t));
        x.ins(I::I32Const((fail == 0) as i32));
        let f = x.rt("rt_bs_start");
        x.ins(I::Call(f));
        x.ins(I::End);
        x.ins(I::End);
    }
    x.ins(I::Else);
    x.ins(I::LocalGet(t));
    x.ins(I::I32Const((fail == 0) as i32));
    let f = x.rt("rt_bs_start");
    x.ins(I::Call(f));
    x.ins(I::End);
    x.check_or_fail(fail)?;
    let r = x.tmp(2);
    x.ins(I::LocalSet(r));
    x.dst(dst, |x| {
        x.ins(I::LocalGet(r));
        Ok(())
    })
}

pub fn instr(mut xx: Ext, ins: &Instr) -> Result<(), CgError> {
    let x = &mut xx;
    let a = &ins.args;
    match ins.op {
        "bs_start_match3" => {
            // Fail Src Live Dst
            bs_start(x, label(&a[0]), &a[1].clone(), &a[3].clone())?;
        }
        "bs_start_match4" => {
            // Fail(no_fail | resume | label) Live Src Dst
            bs_start(x, label(&a[0]), &a[2].clone(), &a[3].clone())?;
        }
        "bs_get_position" => {
            // the stored small(pos) is the result
            let ctx = a[0].clone();
            x.dst(&a[1].clone(), |x| {
                x.src(&ctx)?;
                x.ins(I::I32WrapI64);
                x.ins(I::I64Load(m64(14)));
                Ok(())
            })?;
        }
        "bs_set_position" => {
            x.src(&a[0])?;
            x.ins(I::I32WrapI64);
            x.src(&a[1])?;
            x.ins(I::I64Store(m64(14)));
        }
        "bs_get_tail" => {
            let ctx = a[0].clone();
            x.dst(&a[1].clone(), |x| {
                x.src(&ctx)?;
                let f = x.rt("rt_bs_peek_tail");
                x.ins(I::Call(f));
                Ok(())
            })?;
        }
        "bs_test_tail2" => {
            let (ctx, bits) = (a[1].clone(), uval(&a[2])?);
            call_test(x, label(&a[0]), |x| {
                x.src(&ctx)?;
                x.ins(I::I64Const(bits as i64));
                Ok(())
            }, "rt_bs_ensure_exactly")?;
        }
        "bs_test_unit" => {
            let (ctx, unit) = (a[1].clone(), uval(&a[2])?);
            call_test(x, label(&a[0]), |x| {
                x.src(&ctx)?;
                x.ins(I::I64Const(unit as i64));
                Ok(())
            }, "rt_bs_test_unit")?;
        }
        "bs_match" => {
            let fail = label(&a[0]);
            let ctx = a[1].clone();
            let Operand::List(cmds) = &a[2] else { return Err(CgError("bad bs_match".into())) };
            let cmds = cmds.clone();
            bs_match(x, fail, &ctx, &cmds)?;
        }
        "bs_get_integer2" | "bs_get_binary2" | "bs_get_float2" => {
            // Fail Ctx Live Size Unit Flags Dst
            let fail = label(&a[0]);
            let (ctx, size, unit) = (a[1].clone(), a[3].clone(), uval(&a[4])?);
            let fl = flags(x, &a[5]);
            let op = ins.op;
            call_term(x, fail, &a[6].clone(), |x| {
                x.src(&ctx)?;
                x.src(&size)?;
                x.ins(I::I64Const(unit as i64));
                if op != "bs_get_binary2" {
                    x.ins(I::I32Const(fl));
                }
                Ok(())
            }, match op {
                "bs_get_integer2" => "rt_bs_get_integer2",
                "bs_get_binary2" => "rt_bs_get_binary2",
                _ => "rt_bs_get_float2",
            })?;
        }
        "bs_skip_bits2" => {
            // Fail Ctx Size Unit Flags
            let (ctx, size, unit) = (a[1].clone(), a[2].clone(), uval(&a[3])?);
            call_test(x, label(&a[0]), |x| {
                x.src(&ctx)?;
                x.src(&size)?;
                x.ins(I::I64Const(unit as i64));
                Ok(())
            }, "rt_bs_skip_bits2")?;
        }
        "bs_match_string" => {
            // Fail Ctx Bits Offset
            let (ctx, bits, off) = (a[1].clone(), uval(&a[2])?, uval(&a[3])? as usize);
            let bytes = x.strings()[off..off + bits.div_ceil(8) as usize].to_vec();
            let li = x.add_literal(binary_etf(&bytes));
            call_test(x, label(&a[0]), |x| {
                x.src(&ctx)?;
                x.ins(I::I64Const(bits as i64));
                x.literal(li);
                Ok(())
            }, "rt_bs_match_string")?;
        }
        "bs_get_utf8" | "bs_get_utf16" | "bs_get_utf32" => {
            // Fail Ctx Live Flags Dst
            let kind = ins.op[10..].parse::<i32>().unwrap();
            let ctx = a[1].clone();
            let fl = flags(x, &a[3]);
            call_term(x, label(&a[0]), &a[4].clone(), |x| {
                x.src(&ctx)?;
                x.ins(I::I32Const(kind));
                x.ins(I::I32Const(fl));
                Ok(())
            }, "rt_bs_get_utf")?;
        }
        "bs_skip_utf8" | "bs_skip_utf16" | "bs_skip_utf32" => {
            let kind = ins.op[11..].parse::<i32>().unwrap();
            let ctx = a[1].clone();
            let fl = flags(x, &a[3]);
            call_test(x, label(&a[0]), |x| {
                x.src(&ctx)?;
                x.ins(I::I32Const(kind));
                x.ins(I::I32Const(fl));
                Ok(())
            }, "rt_bs_skip_utf")?;
        }
        "bs_init_writable" => {
            x.ins(I::LocalGet(0));
            let f = x.rt("rt_bs_init_writable");
            x.ins(I::Call(f));
            x.ins(I::LocalSet(0));
        }
        "bs_create_bin" => {
            // Fail Alloc Live Unit Dst {Type Seg Unit Flags Src Size}...
            let fail = label(&a[0]);
            let Operand::List(segs) = &a[5] else { return Err(CgError("bad bs_create_bin".into())) };
            let segs = segs.clone();
            let n = segs.len() / 6;
            if 5 * n > 500 {
                return Err(CgError("bs_create_bin with too many segments".into()));
            }
            let all = x.atom("all");
            for k in 0..n {
                let s = &segs[6 * k..6 * k + 6];
                let Operand::Atom(t) = &s[0] else { return Err(CgError("bad segment type".into())) };
                let tname = x.atom_name(*t).to_string();
                let unit = uval(&s[2])?;
                let fl = flags(x, &s[3]);
                let base = 5 * k as u64;
                let (code, src, size): (i64, Option<Operand>, Option<Operand>) = match tname.as_str() {
                    "integer" => (0, Some(s[4].clone()), Some(s[5].clone())),
                    "binary" => (1, Some(s[4].clone()), Some(s[5].clone())),
                    "float" => (2, Some(s[4].clone()), Some(s[5].clone())),
                    "string" => {
                        let off = uval(&s[4])? as usize;
                        let len = uval(&s[5])? as usize;
                        let bytes = x.strings()[off..off + len].to_vec();
                        let li = x.add_literal(binary_etf(&bytes));
                        scratch(x, base + 3, |x| {
                            x.literal(li);
                            Ok(())
                        })?;
                        (1, None, None)
                    }
                    "utf8" => (4, Some(s[4].clone()), None),
                    "utf16" => (5, Some(s[4].clone()), None),
                    "utf32" => (6, Some(s[4].clone()), None),
                    "append" => (7, Some(s[4].clone()), None),
                    "private_append" => (8, Some(s[4].clone()), None),
                    other => return Err(CgError(format!("unsupported segment type {}", other))),
                };
                scratch(x, base, |x| {
                    x.ins(I::I64Const(code));
                    Ok(())
                })?;
                scratch(x, base + 1, |x| {
                    x.ins(I::I64Const(if tname == "string" { 8 } else { unit as i64 }));
                    Ok(())
                })?;
                scratch(x, base + 2, |x| {
                    x.ins(I::I64Const(fl as i64));
                    Ok(())
                })?;
                if let Some(src) = src {
                    scratch(x, base + 3, |x| x.src(&src))?;
                }
                match size {
                    Some(sz) if !matches!(sz.plain(), Operand::Atom(_)) => scratch(x, base + 4, |x| x.src(&sz))?,
                    _ => scratch(x, base + 4, |x| {
                        x.ins(I::I64Const(all));
                        Ok(())
                    })?,
                }
            }
            call_term(x, fail, &a[4].clone(), |x| {
                x.ins(I::I32Const(n as i32));
                scratch_addr(x);
                Ok(())
            }, "rt_bs_create")?;
        }

        // maps
        "get_map_elements" => {
            // Fail Src {Key Dst}...
            let fail = label(&a[0]);
            let Operand::List(kd) = &a[2] else { return Err(CgError("bad get_map_elements".into())) };
            let kd = kd.clone();
            let src_t = x.tmp(0);
            x.src(&a[1])?;
            x.ins(I::LocalSet(src_t));
            for p in kd.chunks(2) {
                let key = p[0].clone();
                call_term(x, fail, &p[1], |x| {
                    x.ins(I::LocalGet(src_t));
                    x.src(&key)?;
                    Ok(())
                }, "rt_map_get")?;
            }
        }
        "has_map_fields" => {
            let fail = label(&a[0]);
            let Operand::List(keys) = &a[2] else { return Err(CgError("bad has_map_fields".into())) };
            for k in keys.clone() {
                let src = a[1].clone();
                call_test(x, fail, |x| {
                    x.src(&src)?;
                    x.src(&k)
                }, "rt_map_has")?;
            }
        }
        "put_map_assoc" | "put_map_exact" => {
            // Fail Src Dst Live {K V}...
            let fail = label(&a[0]);
            let Operand::List(kv) = &a[4] else { return Err(CgError("bad put_map".into())) };
            let kv = kv.clone();
            if kv.len() > 500 {
                return Err(CgError("put_map with too many pairs".into()));
            }
            for (i, o) in kv.iter().enumerate() {
                let o = o.clone();
                scratch(x, i as u64, |x| x.src(&o))?;
            }
            let src = a[1].clone();
            let exact = (ins.op == "put_map_exact") as i32;
            let n = (kv.len() / 2) as i32;
            call_term(x, fail, &a[2].clone(), |x| {
                x.src(&src)?;
                x.ins(I::I32Const(n));
                scratch_addr(x);
                x.ins(I::I32Const(exact));
                Ok(())
            }, "rt_map_put")?;
        }
        "is_any_native_record" => {
            x.ins(I::I32Const(0));
            x.test_jump(label(&a[0]))?;
        }
        other => return Err(CgError(format!("unsupported opcode {}", other))),
    }
    Ok(())
}
