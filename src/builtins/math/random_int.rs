//! Purpose:
//! Home of the PHP `random_int` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `random_int` is a pure-data builtin returning `Int`.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "random_int",
    area: Math,
    params: [min: Int, max: Int],
    returns: Int,
    lower: lower,
    summary: "Get a cryptographically secure, uniformly selected integer.",
    php_manual: "https://www.php.net/manual/en/function.random-int.php",
}

/// Lowers a `random_int` call by dispatching to the shared cryptographic-random emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::math::lower_random_int(ctx, inst)
}
