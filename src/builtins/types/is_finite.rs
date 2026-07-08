//! Purpose:
//! Home of the PHP `is_finite` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - The parameter is named `num` (matching the PHP golden signature), not `value`.
//! - `lower` is a thin wrapper over the EIR math-module finite-predicate emitter.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "is_finite",
    area: Types,
    params: [num: Float],
    returns: Bool,
    lower: lower,
    summary: "Checks whether a float is finite.",
    php_manual: "function.is-finite",
}

/// Lowers an `is_finite` call by dispatching to the EIR math-module finite-predicate emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::math::lower_is_finite(ctx, inst)
}
