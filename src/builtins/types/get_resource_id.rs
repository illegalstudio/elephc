//! Purpose:
//! Home of the PHP `get_resource_id` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - The parameter is named `resource` (matching the PHP golden signature).
//! - `lower` is a thin wrapper over the EIR types-module resource-id emitter.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "get_resource_id",
    area: Types,
    params: [resource: Mixed],
    returns: Int,
    lower: lower,
    summary: "Returns an integer identifier for the given resource.",
    php_manual: "function.get-resource-id",
}

/// Lowers a `get_resource_id` call by dispatching to the EIR types-module emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::types::lower_get_resource_id(ctx, inst)
}
