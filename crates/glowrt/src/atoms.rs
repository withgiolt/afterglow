//! The atom table. Indices below `FIXED.len()` are fixed (atoms.txt, shared with
//! glowc); the compiler appends every atom the program uses; `binary_to_atom` and
//! friends append at run time.

use crate::term::*;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

#[derive(Default)]
pub struct Fx(u64);
impl Hasher for Fx {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ b as u64).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }
}
pub type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<Fx>>;

pub const FIXED_SRC: &str = include_str!("atoms.txt");

macro_rules! fixed {
    ($($name:ident = $i:expr),* $(,)?) => { $(pub const $name: Term = (($i as u64) << 6) | 0x0B;)* };
}
fixed! {
    FALSE = 0, TRUE = 1, OK = 2, ERROR = 3, UNDEFINED = 4, NIL_ATOM = 5, BADARG = 6,
    BADARITH = 7, BADMATCH = 8, CASE_CLAUSE = 9, FUNCTION_CLAUSE = 10, IF_CLAUSE = 11,
    TRY_CLAUSE = 12, UNDEF = 13, BADFUN = 14, BADARITY = 15, BADKEY = 16, BADMAP = 17,
    BADRECORD = 18, SYSTEM_LIMIT = 19, THROW = 20, EXIT = 21, EXIT_UPPER = 22, NOCATCH = 23,
    NORMAL = 24, INFINITY = 25, UTF8 = 26, LATIN1 = 27, UNICODE = 28, BIG = 29, LITTLE = 30,
    SIGNED = 31, UNSIGNED = 32, NATIVE = 33, BINARY = 34, INTEGER = 35, FLOAT = 36,
    MODULE = 37, NAME = 38, ARITY = 39, ENV = 40, TYPE = 41, LOCAL = 42, EXTERNAL = 43,
    NOTSUP = 44, ALL = 45, NOMATCH = 46, INCOMPLETE = 47, GLOBAL = 48, SCOPE = 49,
    NEW_INDEX = 50, NEW_UNIQ = 51, INDEX = 52, UNIQ = 53, PID = 54, NONODE = 55,
    NOPROC = 56, ERLANG = 57, APPLY = 58, WARMUP = 59, DECIMALS = 60, SCIENTIFIC = 61,
    SHORT = 62, COMPACT = 63, MIN = 64, MAX = 65, SIZE = 66, USED = 67, ITERATOR = 68,
    NONE = 69, ORDERED = 70, REVERSED = 71, DOWN = 72, PROCESS = 73, KILL = 74, KILLED = 75,
    TRAP_EXIT = 76, NOCONNECTION = 77, FLUSH = 78, MESSAGE_QUEUE_LEN = 79, LINKS = 80,
    STATUS = 81, WAITING = 82, RUNNING = 83, RUNNABLE = 84, REGISTERED_NAME = 85,
    DICTIONARY = 86, MONITORS = 87, TIMEOUT_VALUE = 88,
}

pub struct Atoms {
    pub names: Vec<String>,
    pub index: FxMap<String, u32>,
}

pub static mut ATOMS: Atoms = Atoms { names: Vec::new(), index: HashMap::with_hasher(BuildHasherDefault::new()) };

pub fn table() -> &'static mut Atoms {
    unsafe { &mut *core::ptr::addr_of_mut!(ATOMS) }
}

/// Loads the compiler's atom table: names separated by NUL bytes.
pub fn load(blob: &[u8]) {
    let t = table();
    for name in blob.split(|&b| b == 0) {
        let s = String::from_utf8_lossy(name).into_owned();
        let i = t.names.len() as u32;
        t.index.entry(s.clone()).or_insert(i);
        t.names.push(s);
    }
    debug_assert_eq!(t.names[1], "true");
}

pub fn intern(name: &str) -> Term {
    let t = table();
    if let Some(&i) = t.index.get(name) {
        return atom(i);
    }
    let i = t.names.len() as u32;
    t.names.push(name.to_string());
    t.index.insert(name.to_string(), i);
    atom(i)
}

pub fn lookup(name: &str) -> Option<Term> {
    table().index.get(name).map(|&i| atom(i))
}

pub fn name(a: Term) -> &'static str {
    let t = table();
    &t.names[atom_index(a) as usize]
}

pub fn boolean(b: bool) -> Term {
    if b {
        TRUE
    } else {
        FALSE
    }
}
