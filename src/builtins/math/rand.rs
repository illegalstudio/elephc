//! Purpose:
//! Home of the PHP `rand` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `min_args: 0` allows 0-arg calls (returns a raw random u32) in addition to
//!   the 2-arg range form.
//! - A `check` hook rejects exactly 1 argument, matching PHP's "0 or 2 arguments" rule.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "rand",
    area: Math,
    params: [min: Int, max: Int],
    min_args: 0,
    returns: Int,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Rand,
    ),
    summary: "Generate a random integer.",
    php_manual: "https://www.php.net/manual/en/function.rand.php",
}

/// Rejects exactly 1 argument, matching PHP's "0 or 2 arguments" arity rule.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if cx.args.len() == 1 {
        return Err(CompileError::new(cx.span, "rand() takes 0 or 2 arguments"));
    }
    Ok(PhpType::Int)
}
