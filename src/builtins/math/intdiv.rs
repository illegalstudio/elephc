//! Purpose:
//! Home of the PHP `intdiv` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `intdiv` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "intdiv",
    area: Math,
    params: [num1: Int, num2: Int],
    returns: Int,
    lower: lower,
    summary: "Integer division.",
    php_manual: "https://www.php.net/manual/en/function.intdiv.php",
}

/// Lowers an `intdiv` call by dispatching to the shared integer-division emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::math::lower_intdiv(ctx, inst)
}
