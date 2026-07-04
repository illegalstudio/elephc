//! Purpose:
//! Home of the PHP `pi` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `pi` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration. It takes no arguments.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "pi",
    area: Math,
    params: [],
    returns: Float,
    lower: lower,
    summary: "Gets value of pi.",
    php_manual: "https://www.php.net/manual/en/function.pi.php",
}

/// Lowers a `pi` call by dispatching to the shared pi-constant emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::math::lower_pi(ctx, inst)
}
