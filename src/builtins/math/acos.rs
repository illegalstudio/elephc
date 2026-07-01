//! Purpose:
//! Home of the PHP `acos` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `acos` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "acos",
    area: Math,
    params: [num: Float],
    returns: Float,
    lower: lower,
    summary: "Returns the arccosine of a number in radians.",
    php_manual: "https://www.php.net/manual/en/function.acos.php",
}

/// Lowers a `acos` call by dispatching to the libm emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "acos")
}
