//! Binary, map and record instructions.

use crate::codegen::{CgError, Ext};
use beam_reader::Instr;

pub fn instr(_x: Ext, ins: &Instr) -> Result<(), CgError> {
    Err(CgError(format!("unsupported opcode {}", ins.op)))
}
