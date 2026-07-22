//! Purpose:
//! Home of the PHP `realpath_cache_get` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `AssocArray{Str, Mixed}` to reflect the cache map structure.
//! - `arity_error` is overridden to preserve the legacy message
//!   "realpath_cache_get() takes exactly 0 arguments" (the registry default for
//!   0-arg builtins produces "takes no arguments").

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "realpath_cache_get",
    area: Io,
    params: [],
    arity_error: "realpath_cache_get() takes exactly 0 arguments",
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::RealpathCacheGet,
    ),
    summary: "Returns realpath cache entries.",
    php_manual: "function.realpath-cache-get",
}

/// Returns `AssocArray{Str, Mixed}` reflecting the realpath cache structure.
///
/// The registry enforces 0-argument arity via `arity_error` before calling this hook.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
}
