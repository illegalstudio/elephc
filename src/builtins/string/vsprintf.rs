//! Purpose:
//! Home of the PHP `vsprintf` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   all via `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts a required `format` string and a `values` array.
//! - `lower` is a thin wrapper over the shared `lower_vsprintf` emitter.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "vsprintf",
    area: String,
    params: [format: Str, values: Mixed],
    returns: Str,
    lower: lower,
    summary: "Returns a formatted string using an array of values.",
    php_manual: "https://www.php.net/manual/en/function.vsprintf.php",
}

/// Lowers a `vsprintf` call by dispatching to the shared vsprintf emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_vsprintf(ctx, inst)
}
