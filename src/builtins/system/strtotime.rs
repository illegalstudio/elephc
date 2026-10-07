//! Purpose:
//! Home of the PHP `strtotime` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` always returns `Union(Int, Bool)` to reflect PHP's behaviour where
//!   `strtotime` returns a Unix timestamp on success or `false` on failure.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "strtotime",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Strtotime,
    ),
    requirements: crate::builtins::semantics::timezone_validation_requirements,
}

/// Returns `Union(Int, Bool)` to reflect that `strtotime` can return a timestamp or `false`.
///
/// The registry pre-infers arguments before calling this hook.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::False]))
}
