//! Purpose:
//! Home of the PHP `log10` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `log10` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "log10",
    area: Math,
    params: [num: Float],
    returns: Float,
    lower: lower,
    summary: "Returns the base-10 logarithm of a number.",
    php_manual: "https://www.php.net/manual/en/function.log10.php",
}

/// Lowers a `log10` call by dispatching to the libm emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "log10")
}
