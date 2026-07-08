//! Purpose:
//! Home of the PHP `cos` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `cos` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "cos",
    area: Math,
    params: [num: Float],
    returns: Float,
    lower: lower,
    summary: "Returns the cosine of a number (radians).",
    php_manual: "https://www.php.net/manual/en/function.cos.php",
}

/// Lowers a `cos` call by dispatching to the libm emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "cos")
}
