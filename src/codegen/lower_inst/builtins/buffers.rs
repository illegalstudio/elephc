//! Purpose:
//! Implements typed `RuntimeFnId` buffer operations using the direct buffer opcode helpers.
//! Keeps registry semantics separate from physical buffer layout and runtime symbols.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` for `BufferLen` and `BufferFree`.
//!
//! Key details:
//! - Buffer helpers use shared runtime symbols so fatal behavior and buffer
//!   header layout remain consistent.

use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::buffers;
use super::super::super::context::FunctionContext;

/// Lowers `buffer_len()` through the direct buffer opcode helper.
pub(crate) fn lower_buffer_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    buffers::lower_buffer_len(ctx, inst)
}

/// Lowers `buffer_free()` through the direct buffer opcode helper.
pub(crate) fn lower_buffer_free(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    buffers::lower_buffer_free(ctx, inst)
}
