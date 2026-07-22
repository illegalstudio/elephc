//! Purpose:
//! Home of the PHP `stat` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `assoc-array<mixed, int>|bool` via `stat_result_type`, reflecting
//!   PHP behaviour where `stat` returns the stat buffer array on success or `false` on failure.
//! - The registry pre-infers arguments before calling this hook.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "stat",
    area: Io,
    params: [filename: Str],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Stat,
    ),
    summary: "Gives information about a file.",
    php_manual: "function.stat",
}

/// Returns `assoc-array<mixed, int>|bool` reflecting that `stat` returns a buffer or `false`.
///
/// The registry pre-infers arguments before calling this hook.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(crate::builtins::io::stat_support::stat_result_type(cx.checker))
}
