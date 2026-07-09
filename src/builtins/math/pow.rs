//! Purpose:
//! Home of the PHP `pow` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `pow` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "pow",
    area: Math,
    params: [num: Mixed, exponent: Mixed],
    returns: Float,
    lower: lower,
    summary: "Exponential expression.",
    php_manual: "https://www.php.net/manual/en/function.pow.php",
}

/// Lowers a `pow` call by dispatching to the shared exponentiation emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::math::lower_pow(ctx, inst)
}
