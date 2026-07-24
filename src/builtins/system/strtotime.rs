//! Purpose:
//! Home of the PHP `strtotime` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` always returns `Union(Int, Bool)` to reflect PHP's behaviour where
//!   `strtotime` returns a Unix timestamp on success or `false` on failure.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "strtotime",
    area: System,
    params: [datetime: Str, baseTimestamp: Int = DefaultSpec::Null],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Strtotime,
    ),
    requirements: crate::builtins::semantics::windows_timezone_requirements,
    summary: "Parses an English textual datetime description into a Unix timestamp.",
}

/// Returns `Union(Int, Bool)` to reflect that `strtotime` can return a timestamp or `false`.
///
/// The registry pre-infers arguments before calling this hook.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::False]))
}
