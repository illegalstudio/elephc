//! Purpose:
//! Home of the PHP `atan2` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `atan2` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "atan2",
    area: Math,
    params: [y: Float, x: Float],
    returns: Float,
    lower: lower,
    summary: "Returns the arc tangent of two variables.",
    php_manual: "https://www.php.net/manual/en/function.atan2.php",
}

/// Lowers an `atan2` call by dispatching to the shared libm two-argument emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::math::lower_atan2(ctx, inst)
}
