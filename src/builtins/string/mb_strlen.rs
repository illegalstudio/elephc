//! Purpose:
//! Home of the PHP `mb_strlen` builtin: declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), both via
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook needed: `returns: Int` expresses the return type inline; the count is a
//!   pure UTF-8 code-point scan in `__rt_mb_strlen`. UTF-8 is assumed (the AIC/default
//!   encoding); the optional encoding argument is not supported.
//! - Arity (exactly 1 arg) is validated by the registry.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "mb_strlen",
    area: String,
    params: [string: Str],
    returns: Int,
    lower: lower,
    summary: "Counts the UTF-8 code points in a string (multibyte-aware string length).",
    php_manual: "https://www.php.net/manual/en/function.mb-strlen.php",
}

/// Lowers an `mb_strlen` call by dispatching to the shared `lower_mb_strlen` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_mb_strlen(ctx, inst)
}
