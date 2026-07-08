//! Purpose:
//! Home of the PHP `intval` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - Declared with exactly one parameter `value` (no `base` param) matching the legacy golden signature.
//! - `lower` is a thin wrapper over the shared intval emitter.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "intval",
    area: Types,
    params: [value: Mixed],
    returns: Int,
    lower: lower,
    summary: "Returns the integer value of a variable.",
    php_manual: "function.intval",
}

/// Lowers an `intval` call by dispatching to the shared intval emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::lower_intval(ctx, inst)
}
