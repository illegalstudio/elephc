//! Purpose:
//! Home of the PHP `lstat` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `assoc-array<mixed, int>|bool` via `stat_result_type`, reflecting
//!   PHP behaviour where `lstat` returns the stat buffer array on success or `false` on failure.
//!   Unlike `stat`, `lstat` does not follow symbolic links.
//! - The registry pre-infers arguments before calling this hook.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "lstat",
    area: Io,
    params: [filename: Str],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Lstat,
    ),
    summary: "Gives information about a file or symbolic link.",
    php_manual: "function.lstat",
}

/// Returns `assoc-array<mixed, int>|bool` reflecting that `lstat` returns a buffer or `false`.
///
/// The registry pre-infers arguments before calling this hook.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(crate::builtins::io::stat_support::stat_result_type(cx.checker))
}
