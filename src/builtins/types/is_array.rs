//! Purpose:
//! Home of the PHP `is_array` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - `lower` is a thin wrapper over the shared array-predicate emitter.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "is_array",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    lower: lower,
    summary: "Checks whether a variable is an array.",
    php_manual: "function.is-array",
}

/// Lowers an `is_array` call by dispatching to the shared array-predicate emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::lower_is_array(ctx, inst)
}
