//! Purpose:
//! Home of the PHP `ceil` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `ceil` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "ceil",
    area: Math,
    params: [num: Float],
    returns: Float,
    lower: lower,
    summary: "Rounds a number up to the nearest integer.",
    php_manual: "https://www.php.net/manual/en/function.ceil.php",
}

/// Lowers a `ceil` call by dispatching to the shared float-rounding emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::math::lower_ceil(ctx, inst)
}
