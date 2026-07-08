//! Purpose:
//! Routes PHP buffer builtins emitted as EIR `BuiltinCall` instructions.
//! Keeps builtin-name dispatch separate from direct buffer opcodes like `BufferNew`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Buffer helpers use shared runtime symbols so fatal behavior and buffer
//!   header layout remain consistent.

use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::buffers;
use super::super::super::context::FunctionContext;

/// Lowers `buffer_len()` through the direct buffer opcode helper.
pub(super) fn lower_buffer_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    buffers::lower_buffer_len(ctx, inst)
}

/// Lowers `buffer_free()` through the direct buffer opcode helper.
pub(super) fn lower_buffer_free(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    buffers::lower_buffer_free(ctx, inst)
}
