//! Purpose:
//! Home of the PHP `hypot` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `hypot` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "hypot",
    area: Math,
    params: [x: Float, y: Float],
    returns: Float,
    lower: lower,
    summary: "Calculates the length of the hypotenuse of a right-angle triangle.",
    php_manual: "https://www.php.net/manual/en/function.hypot.php",
}

/// Lowers a `hypot` call by dispatching to the shared libm two-argument emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::math::lower_hypot(ctx, inst)
}
