//! Purpose:
//! Home of the PHP `sprintf` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   all via `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts a required `format` string plus a variadic `values` list.
//! - `lower` is a thin wrapper over the shared `lower_sprintf` emitter.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "sprintf",
    area: String,
    params: [format: Str],
    variadic: "values",
    returns: Str,
    lower: lower,
    summary: "Returns a formatted string.",
    php_manual: "https://www.php.net/manual/en/function.sprintf.php",
}

/// Lowers a `sprintf` call by dispatching to the shared sprintf emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_sprintf(ctx, inst)
}
