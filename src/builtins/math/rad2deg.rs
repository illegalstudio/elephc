//! Purpose:
//! Home of the PHP `rad2deg` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `rad2deg` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "rad2deg",
    area: Math,
    params: [num: Float],
    returns: Float,
    lower: lower,
    summary: "Converts a radian value to degrees.",
    php_manual: "https://www.php.net/manual/en/function.rad2deg.php",
}

/// Lowers a `rad2deg` call by multiplying with the 180/PI conversion factor.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::math::lower_rad2deg(ctx, inst)
}
