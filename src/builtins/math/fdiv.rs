//! Purpose:
//! Home of the PHP `fdiv` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `fdiv` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "fdiv",
    area: Math,
    params: [num1: Float, num2: Float],
    returns: Float,
    lower: lower,
    summary: "Divides two numbers, according to IEEE 754.",
    php_manual: "https://www.php.net/manual/en/function.fdiv.php",
}

/// Lowers a `fdiv` call by dispatching to the shared IEEE-754 division emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::math::lower_fdiv(ctx, inst)
}
