//! Purpose:
//! Home of the PHP `floatval` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - `lower` is a thin wrapper over the shared floatval emitter.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "floatval",
    area: Types,
    params: [value: Mixed],
    returns: Float,
    lower: lower,
    summary: "Returns the float value of a variable.",
    php_manual: "function.floatval",
}

/// Lowers a `floatval` call by dispatching to the shared floatval emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::lower_floatval(ctx, inst)
}
