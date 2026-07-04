//! Purpose:
//! Home of the PHP `is_callable` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - `lower` is a thin wrapper over the shared callable-predicate emitter.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "is_callable",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    lower: lower,
    summary: "Checks whether a variable can be called as a function.",
    php_manual: "function.is-callable",
}

/// Lowers an `is_callable` call by dispatching to the shared callable-predicate emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::lower_is_callable(ctx, inst)
}
