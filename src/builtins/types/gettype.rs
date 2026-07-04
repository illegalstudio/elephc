//! Purpose:
//! Home of the PHP `gettype` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - `lower` is a thin wrapper over the shared gettype emitter.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "gettype",
    area: Types,
    params: [value: Mixed],
    returns: Str,
    lower: lower,
    summary: "Returns the type of a variable as a string.",
    php_manual: "function.gettype",
}

/// Lowers a `gettype` call by dispatching to the shared gettype emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::lower_gettype(ctx, inst)
}
