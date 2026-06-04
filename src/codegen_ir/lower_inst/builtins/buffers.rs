//! Purpose:
//! Routes PHP buffer builtins emitted as EIR `BuiltinCall` instructions.
//! Keeps builtin-name dispatch separate from direct buffer opcodes like `BufferNew`.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Buffer helpers use the same runtime symbols as the legacy backend so fatal
//!   behavior and buffer header layout remain shared.

use crate::codegen_ir::Result;
use crate::ir::Instruction;

use super::super::buffers;
use super::super::super::context::FunctionContext;

/// Lowers `buffer_len()` through the direct buffer opcode helper.
pub(super) fn lower_buffer_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    buffers::lower_buffer_len(ctx, inst)
}
