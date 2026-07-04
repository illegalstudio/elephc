//! Purpose:
//! Home of the PHP `is_resource` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - `lower` is a thin wrapper over the EIR types-module resource-predicate emitter.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "is_resource",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    lower: lower,
    summary: "Checks whether a variable is a resource.",
    php_manual: "function.is-resource",
}

/// Lowers an `is_resource` call by dispatching to the EIR types-module resource-predicate emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::types::lower_is_resource(ctx, inst)
}
