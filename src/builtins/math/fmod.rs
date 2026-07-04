//! Purpose:
//! Home of the PHP `fmod` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `fmod` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "fmod",
    area: Math,
    params: [num1: Float, num2: Float],
    returns: Float,
    lower: lower,
    summary: "Returns the floating point remainder of the division of the arguments.",
    php_manual: "https://www.php.net/manual/en/function.fmod.php",
}

/// Lowers an `fmod` call by dispatching to the shared floating-remainder emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::math::lower_fmod(ctx, inst)
}
