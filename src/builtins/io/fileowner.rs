//! Purpose:
//! Home of the PHP `fileowner` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Int, Bool)` reflecting PHP behaviour where `fileowner`
//!   returns the numeric user ID of the file owner on success or `false` on failure.
//! - The registry pre-infers arguments before calling this hook.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "fileowner",
    area: Io,
    params: [filename: Str],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fileowner,
    ),
    summary: "Gets file owner.",
    php_manual: "function.fileowner",
}

/// Returns `Union(Int, Bool)` reflecting that `fileowner` can return a user ID or `false`.
///
/// The registry pre-infers arguments before calling this hook.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::Int, PhpType::False]))
}
