//! Purpose:
//! Home of the PHP `is_infinite` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - The parameter is named `num` (matching the PHP golden signature), not `value`.
//! - `lower` is a thin wrapper over the EIR math-module infinite-predicate emitter.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "is_infinite",
    area: Types,
    params: [num: Float],
    returns: Bool,
    lower: lower,
    summary: "Checks whether a float is infinite.",
    php_manual: "function.is-infinite",
}

/// Lowers an `is_infinite` call by dispatching to the EIR math-module infinite-predicate emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::math::lower_is_infinite(ctx, inst)
}
