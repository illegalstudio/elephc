//! Purpose:
//! Home of the PHP `get_resource_type` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - The parameter is named `resource` (matching the PHP golden signature).
//! - `lower` is a thin wrapper over the EIR types-module resource-type emitter.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "get_resource_type",
    area: Types,
    params: [resource: Mixed],
    returns: Str,
    lower: lower,
    summary: "Returns the type of a resource.",
    php_manual: "function.get-resource-type",
}

/// Lowers a `get_resource_type` call by dispatching to the EIR types-module emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::types::lower_get_resource_type(ctx, inst)
}
