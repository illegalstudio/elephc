//! Purpose:
//! Home of the PHP `disk_free_space` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Float, False)` reflecting PHP behaviour where `disk_free_space`
//!   returns the available byte count on success or `false` on failure.
//! - The registry pre-infers arguments before calling this hook.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "disk_free_space",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::DiskFreeSpace,
    ),
}

/// Returns `Union(Float, False)` reflecting that `disk_free_space` can return bytes or `false`.
///
/// This used to declare a plain `Float` — described as "fully determined by its declaration",
/// which it was not: the declaration is what DISCARDED the failure, so an unstatable path
/// answered `0.0`, which is exactly what a full filesystem reports.
///
/// The registry pre-infers arguments before calling this hook.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::Float, PhpType::False]))
}
