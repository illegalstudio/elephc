//! Purpose:
//! Home of the PHP `ob_end_flush` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook when present),
//!   and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - Flushes the top buffer to the parent sink, then pops the stack.
//! - Pure-data builtin: returns `Bool` (`false` when no output buffer is active).
//! - `lower` is a thin wrapper over `output_buffering::lower_ob_end_flush`.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "ob_end_flush",
    area: Io,
    params: [],
    returns: Bool,
    lower: lower,
    summary: "Flushes (sends) the contents of the active output buffer and turns it off.",
    php_manual: "function.ob-end-flush",
}

/// Lowers an `ob_end_flush` call by dispatching to the shared output-buffering emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::output_buffering::lower_ob_end_flush(ctx, inst)
}
