//! Purpose:
//! Home of the PHP `log` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `log` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration.
//! - The second parameter `base` is optional with a default of `M_E`, matching
//!   PHP's `log(num, base = M_E)` signature. The registry enforces 1-2 args.

use crate::builtins::spec::DefaultSpec;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "log",
    area: Math,
    params: [num: Float, base: Float = DefaultSpec::Float(std::f64::consts::E)],
    returns: Float,
    lower: lower,
    summary: "Natural logarithm.",
    php_manual: "https://www.php.net/manual/en/function.log.php",
}

/// Lowers a `log` call by dispatching to the shared logarithm emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::math::lower_log(ctx, inst)
}
