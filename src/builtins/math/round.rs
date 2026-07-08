//! Purpose:
//! Home of the PHP `round` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `round` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration.
//! - The second parameter `precision` is optional with a default of `0`, matching
//!   PHP's `round(num, precision = 0)` signature. The registry enforces 1-2 args.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "round",
    area: Math,
    params: [num: Float, precision: Int = DefaultSpec::Int(0)],
    returns: Float,
    lower: lower,
    summary: "Rounds a float.",
    php_manual: "https://www.php.net/manual/en/function.round.php",
}

/// Lowers a `round` call by dispatching to the shared float-rounding emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::math::lower_round(ctx, inst)
}
