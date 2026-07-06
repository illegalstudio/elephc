//! Purpose:
//! Home of the PHP `mt_rand` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `min_args: 0` allows 0-arg calls (returns a raw random u32) in addition to
//!   the 2-arg range form.
//! - A `check` hook rejects exactly 1 argument, matching PHP's "0 or 2 arguments" rule.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "mt_rand",
    area: Math,
    params: [min: Int, max: Int],
    min_args: 0,
    returns: Int,
    check: check,
    lower: lower,
    summary: "Generate a random value via the Mersenne Twister Random Number Generator.",
    php_manual: "https://www.php.net/manual/en/function.mt-rand.php",
}

/// Rejects exactly 1 argument, matching PHP's "0 or 2 arguments" arity rule.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if cx.args.len() == 1 {
        return Err(CompileError::new(cx.span, "mt_rand() takes 0 or 2 arguments"));
    }
    Ok(PhpType::Int)
}

/// Lowers an `mt_rand` call by dispatching to the shared random-integer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::math::lower_rand(ctx, inst, "mt_rand")
}
