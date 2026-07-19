//! Purpose:
//! Home of the PHP `boolval` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - `lower` is a thin wrapper over the shared boolval emitter.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "boolval",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    lower: lower,
    summary: "Returns the boolean value of a variable.",
    php_manual: "function.boolval",
}

/// Lowers a `boolval` call by dispatching to the shared boolval emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::lower_boolval(ctx, inst)
}
