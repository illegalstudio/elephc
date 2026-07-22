//! Purpose:
//! Home of the PHP `strpos` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The declared signature carries the full golden param list (`haystack`, `needle`,
//!   `offset`), but `max_args: 2` caps `check_arity` so a third argument is rejected,
//!   matching the legacy CHECK arm which enforced exactly two arguments.
//! - `check` returns `PhpType::Union([Int, Bool])` (position, or `false` on no match).
//!   A check hook is required because the `builtin!` macro `returns:` field only accepts
//!   a simple type identifier and cannot express a union inline. Argument types are
//!   inferred by the common registry dispatch path before the hook fires.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "strpos",
    area: String,
    params: [haystack: Str, needle: Str, offset: Int = DefaultSpec::Int(0)],
    max_args: 2,
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Strpos,
    ),
    summary: "Finds the numeric position of the first occurrence of a substring.",
    php_manual: "https://www.php.net/manual/en/function.strpos.php",
}

/// Returns `PhpType::Union([Int, Bool])` for a `strpos` call (position, or `false`).
///
/// A check hook is required because the `builtin!` macro cannot express a union return
/// type inline. Argument types are inferred by the common registry dispatch path before
/// this hook fires; arity (capped to 2 via `max_args`) is validated by the registry.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::False]))
}
