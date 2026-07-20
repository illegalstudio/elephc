//! Purpose:
//! Home of the PHP `ob_get_level` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook when present),
//!   and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Int`, the nesting depth, 0 = no buffering)
//! -   is fully determined by the declaration.
//! - `lower` is a thin wrapper over `output_buffering::lower_ob_get_level`.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "ob_get_level",
    area: Io,
    params: [],
    returns: Int,
    lower: lower,
    summary: "Returns the nesting level of the output buffering mechanism.",
    php_manual: "function.ob-get-level",
}

/// Lowers an `ob_get_level` call by dispatching to the shared output-buffering emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::output_buffering::lower_ob_get_level(ctx, inst)
}
