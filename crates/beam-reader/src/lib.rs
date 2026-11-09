//! Reads a `.beam` file: the IFF chunks, the atom, import, export, fun and literal
//! tables, and the `Code` chunk decoded from BEAM's compact term encoding into a flat
//! list of instructions per function.

mod opcodes;
pub use opcodes::{OPCODES, OPCODE_MAX};

use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

type Result<T> = std::result::Result<T, Error>;

fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error(msg.into()))
}

/// An instruction operand, as encoded in the `Code` chunk.
#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    /// `u`: an unsigned literal (arities, counts, sizes, flags).
    U(u64),
    /// `i`: an integer that fits in 64 bits.
    I(i64),
    /// `i`: a larger integer, big-endian two's complement bytes.
    BigI(Vec<u8>),
    /// `a`: an atom, 1-based index into the module's atom table. 0 means `[]`.
    Atom(u32),
    Nil,
    X(u32),
    Y(u32),
    /// `f`: a label. 0 means "no label" (raise instead of jump).
    Label(u32),
    /// `h`: a character.
    Char(u32),
    List(Vec<Operand>),
    FloatReg(u32),
    AllocList(Vec<(u64, u64)>),
    /// Index into the literal table.
    Literal(u32),
    /// A register annotated with a type index (OTP 25+); the type is ignored.
    TypedReg(Box<Operand>, u32),
}

impl Operand {
    /// Strips type annotations.
    pub fn plain(&self) -> &Operand {
        match self {
            Operand::TypedReg(r, _) => r.plain(),
            o => o,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Instr {
    pub op: &'static str,
    pub args: Vec<Operand>,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub name: u32,
    pub arity: u32,
    /// Label of the entry point.
    pub entry: u32,
    pub code: Vec<Instr>,
}

#[derive(Clone, Debug)]
pub struct Import {
    pub module: u32,
    pub function: u32,
    pub arity: u32,
}

#[derive(Clone, Debug)]
pub struct Export {
    pub function: u32,
    pub arity: u32,
    pub label: u32,
}

#[derive(Clone, Debug)]
pub struct Lambda {
    pub function: u32,
    pub arity: u32,
    pub label: u32,
    pub index: u32,
    pub num_free: u32,
}

#[derive(Debug)]
pub struct Module {
    /// atoms[0] is the module name (atom index 1).
    pub atoms: Vec<String>,
    pub imports: Vec<Import>,
    pub exports: Vec<Export>,
    pub locals: Vec<Export>,
    pub lambdas: Vec<Lambda>,
    /// Each literal as an external term format blob, including the 131 version byte.
    pub literals: Vec<Vec<u8>>,
    pub strings: Vec<u8>,
    pub functions: Vec<Function>,
    pub instruction_set: u32,
}

impl Module {
    pub fn name(&self) -> &str {
        &self.atoms[0]
    }
    /// Atom by 1-based index, as used in operands and tables.
    pub fn atom(&self, index: u32) -> &str {
        &self.atoms[index as usize - 1]
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }
    fn at_end(&self) -> bool {
        self.pos >= self.data.len()
    }
    fn u8(&mut self) -> Result<u8> {
        match self.data.get(self.pos) {
            Some(b) => {
                self.pos += 1;
                Ok(*b)
            }
            None => err("unexpected end of data"),
        }
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return err("unexpected end of data");
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    /// One compact-encoded term. Tags: 0 u, 1 i, 2 a, 3 x, 4 y, 5 f, 6 h, 7 extended.
    fn operand(&mut self) -> Result<Operand> {
        let b = self.u8()?;
        let tag = b & 7;
        if tag == 7 {
            return self.extended(b >> 4);
        }
        let value = self.value(b, tag == 1)?;
        Ok(match (tag, value) {
            (0, Value::Small(v)) => Operand::U(v as u64),
            (1, Value::Small(v)) => Operand::I(v),
            (1, Value::Big(bytes)) => Operand::BigI(bytes),
            (2, Value::Small(0)) => Operand::Nil,
            (2, Value::Small(v)) => Operand::Atom(v as u32),
            (3, Value::Small(v)) => Operand::X(v as u32),
            (4, Value::Small(v)) => Operand::Y(v as u32),
            (5, Value::Small(v)) => Operand::Label(v as u32),
            (6, Value::Small(v)) => Operand::Char(v as u32),
            (t, v) => return err(format!("bad operand tag {} value {:?}", t, v)),
        })
    }

    fn unsigned(&mut self) -> Result<u64> {
        let b = self.u8()?;
        match self.value(b, false)? {
            Value::Small(v) => Ok(v as u64),
            Value::Big(_) => err("unsigned value too large"),
        }
    }

    fn value(&mut self, b: u8, signed: bool) -> Result<Value> {
        if b & 0x08 == 0 {
            return Ok(Value::Small((b >> 4) as i64));
        }
        if b & 0x10 == 0 {
            let lo = self.u8()? as i64;
            return Ok(Value::Small((((b >> 5) as i64) << 8) | lo));
        }
        let n = if (b >> 5) != 7 {
            ((b >> 5) + 2) as usize
        } else {
            self.unsigned()? as usize + 9
        };
        let bytes = self.bytes(n)?.to_vec();
        if n <= 8 {
            let mut v: i64 = if signed && bytes[0] & 0x80 != 0 { -1 } else { 0 };
            for byte in &bytes {
                v = (v << 8) | *byte as i64;
            }
            if !signed && n == 8 && bytes[0] & 0x80 != 0 {
                return err("unsigned value does not fit in i64");
            }
            Ok(Value::Small(v))
        } else {
            Ok(Value::Big(bytes))
        }
    }

    fn extended(&mut self, ext: u8) -> Result<Operand> {
        match ext {
            1 => {
                let n = self.unsigned()?;
                let mut items = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    items.push(self.operand()?);
                }
                Ok(Operand::List(items))
            }
            2 => Ok(Operand::FloatReg(self.unsigned()? as u32)),
            3 => {
                let n = self.unsigned()?;
                let mut items = Vec::new();
                for _ in 0..n {
                    let kind = self.unsigned()?;
                    let val = self.unsigned()?;
                    items.push((kind, val));
                }
                Ok(Operand::AllocList(items))
            }
            4 => Ok(Operand::Literal(self.unsigned()? as u32)),
            5 => {
                let reg = self.operand()?;
                let ty = self.unsigned()? as u32;
                Ok(Operand::TypedReg(Box::new(reg), ty))
            }
            e => err(format!("unknown extended operand tag {}", e)),
        }
    }
}

#[derive(Debug)]
enum Value {
    Small(i64),
    Big(Vec<u8>),
}

pub fn read(data: &[u8]) -> Result<Module> {
    let mut r = Reader::new(data);
    if r.bytes(4)? != b"FOR1" {
        return err("not an IFF file");
    }
    let _size = r.u32()?;
    if r.bytes(4)? != b"BEAM" {
        return err("not a BEAM file");
    }
    let mut chunks: Vec<(&[u8], &[u8])> = Vec::new();
    while !r.at_end() {
        let id = r.bytes(4)?;
        let len = r.u32()? as usize;
        let body = r.bytes(len)?;
        let pad = (4 - len % 4) % 4;
        r.pos = (r.pos + pad).min(data.len());
        chunks.push((id, body));
    }
    let chunk = |id: &[u8]| chunks.iter().find(|(i, _)| *i == id).map(|(_, b)| *b);

    let atoms = match chunk(b"AtU8").or_else(|| chunk(b"Atom")) {
        Some(b) => read_atoms(b)?,
        None => return err("no atom chunk"),
    };
    let imports = chunk(b"ImpT").map(read_triples).transpose()?.unwrap_or_default()
        .into_iter().map(|(m, f, a)| Import { module: m, function: f, arity: a }).collect();
    let exports = chunk(b"ExpT").map(read_triples).transpose()?.unwrap_or_default()
        .into_iter().map(|(f, a, l)| Export { function: f, arity: a, label: l }).collect();
    let locals = chunk(b"LocT").map(read_triples).transpose()?.unwrap_or_default()
        .into_iter().map(|(f, a, l)| Export { function: f, arity: a, label: l }).collect();
    let lambdas = match chunk(b"FunT") {
        Some(b) => read_lambdas(b)?,
        None => Vec::new(),
    };
    let literals = match chunk(b"LitT") {
        Some(b) => read_literals(b)?,
        None => Vec::new(),
    };
    let strings = chunk(b"StrT").map(|b| b.to_vec()).unwrap_or_default();
    let code = chunk(b"Code").ok_or_else(|| Error("no Code chunk".into()))?;
    let (instruction_set, functions) = read_code(code)?;

    Ok(Module { atoms, imports, exports, locals, lambdas, literals, strings, functions, instruction_set })
}

fn read_atoms(b: &[u8]) -> Result<Vec<String>> {
    let mut r = Reader::new(b);
    let count = r.u32()? as i32;
    let (count, compact) = if count < 0 { (-count, true) } else { (count, false) };
    let mut atoms = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let len = if compact {
            let b = r.u8()?;
            match r.value(b, false)? {
                Value::Small(v) => v as usize,
                Value::Big(_) => return err("atom length too large"),
            }
        } else {
            r.u8()? as usize
        };
        let s = r.bytes(len)?;
        atoms.push(String::from_utf8(s.to_vec()).map_err(|_| Error("atom is not UTF-8".into()))?);
    }
    Ok(atoms)
}

fn read_triples(b: &[u8]) -> Result<Vec<(u32, u32, u32)>> {
    let mut r = Reader::new(b);
    let n = r.u32()?;
    let mut v = Vec::with_capacity(n as usize);
    for _ in 0..n {
        v.push((r.u32()?, r.u32()?, r.u32()?));
    }
    Ok(v)
}

fn read_lambdas(b: &[u8]) -> Result<Vec<Lambda>> {
    let mut r = Reader::new(b);
    let n = r.u32()?;
    let mut v = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let function = r.u32()?;
        let arity = r.u32()?;
        let label = r.u32()?;
        let index = r.u32()?;
        let num_free = r.u32()?;
        let _old_uniq = r.u32()?;
        v.push(Lambda { function, arity, label, index, num_free });
    }
    Ok(v)
}

fn read_literals(b: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut r = Reader::new(b);
    let uncompressed = r.u32()? as usize;
    let rest = &b[4..];
    let data = if uncompressed == 0 {
        rest.to_vec()
    } else {
        miniz_oxide::inflate::decompress_to_vec_zlib(rest)
            .map_err(|e| Error(format!("cannot inflate LitT: {:?}", e)))?
    };
    let mut r = Reader::new(&data);
    let n = r.u32()?;
    let mut lits = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let len = r.u32()? as usize;
        lits.push(r.bytes(len)?.to_vec());
    }
    Ok(lits)
}

fn read_code(b: &[u8]) -> Result<(u32, Vec<Function>)> {
    let mut r = Reader::new(b);
    let header_len = r.u32()? as usize;
    let header = r.bytes(header_len)?;
    let mut h = Reader::new(header);
    let instruction_set = h.u32()?;
    let opcode_max = h.u32()? as usize;
    if opcode_max > OPCODE_MAX {
        return err(format!(
            "module uses opcode {} but this reader knows up to {} (newer OTP?)",
            opcode_max, OPCODE_MAX
        ));
    }
    let mut functions: Vec<Function> = Vec::new();
    let mut current: Option<Function> = None;
    // Labels seen since the last func_info belong to the next function.
    let mut pending: Vec<Instr> = Vec::new();
    while !r.at_end() {
        let opcode = r.u8()? as usize;
        if opcode == 0 || opcode > OPCODE_MAX {
            return err(format!("bad opcode {}", opcode));
        }
        let (name, arity) = OPCODES[opcode];
        let mut args = Vec::with_capacity(arity as usize);
        for _ in 0..arity {
            args.push(r.operand()?);
        }
        let instr = Instr { op: name, args };
        match name {
            "int_code_end" => break,
            // Buffered until the next real instruction: labels and line markers right
            // before func_info belong to the next function (its clause-failure label).
            "label" | "line" | "executable_line" => pending.push(instr),
            "func_info" => {
                if let Some(f) = current.take() {
                    functions.push(f);
                }
                let (name, arity) = match (&instr.args[1], &instr.args[2]) {
                    (Operand::Atom(f), Operand::U(a)) => (*f, *a as u32),
                    _ => return err("bad func_info"),
                };
                let mut code = std::mem::take(&mut pending);
                code.push(instr);
                current = Some(Function { name, arity, entry: 0, code });
            }
            _ => match current.as_mut() {
                Some(f) => {
                    f.code.append(&mut pending);
                    f.code.push(instr);
                }
                None => return err(format!("instruction {} outside a function", name)),
            },
        }
    }
    if let Some(f) = current.take() {
        functions.push(f);
    }
    // The entry label is the label right after func_info.
    for f in &mut functions {
        let fi = f.code.iter().position(|i| i.op == "func_info").unwrap();
        f.entry = f.code[fi + 1..]
            .iter()
            .find_map(|i| match (i.op, i.args.first()) {
                ("label", Some(Operand::U(l))) => Some(*l as u32),
                _ => None,
            })
            .ok_or_else(|| Error("function without an entry label".into()))?;
    }
    Ok((instruction_set, functions))
}

