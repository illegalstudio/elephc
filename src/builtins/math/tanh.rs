//! Purpose:
//! Home of the PHP `tanh` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `tanh` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "tanh",
    area: Math,
    params: [num: Float],
    returns: Float,
    lower: lower,
    summary: "Returns the hyperbolic tangent of a number.",
    php_manual: "https://www.php.net/manual/en/function.tanh.php",
}

/// Lowers a `tanh` call by dispatching to the libm emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "tanh")
}
