//! Purpose:
//! Home of the PHP `deg2rad` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `deg2rad` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "deg2rad",
    area: Math,
    params: [num: Float],
    returns: Float,
    lower: lower,
    summary: "Converts a degree value to radians.",
    php_manual: "https://www.php.net/manual/en/function.deg2rad.php",
}

/// Lowers a `deg2rad` call by multiplying with the PI/180 conversion factor.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::math::lower_deg2rad(ctx, inst)
}
