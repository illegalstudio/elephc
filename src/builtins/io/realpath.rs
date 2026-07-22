//! Purpose:
//! Home of the PHP `realpath` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Str, Bool)` to reflect PHP's behaviour where `realpath`
//!   returns the resolved path on success or `false` if the path cannot be resolved.
//! - The registry pre-infers arguments before calling this hook.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "realpath",
    area: Io,
    params: [path: Str],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Realpath,
    ),
    summary: "Returns canonicalized absolute pathname.",
    php_manual: "function.realpath",
}

/// Returns `Union(Str, Bool)` reflecting that `realpath` can return a path or `false`.
///
/// The registry pre-infers arguments before calling this hook.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}
