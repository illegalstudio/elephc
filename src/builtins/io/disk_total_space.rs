//! Purpose:
//! Home of the PHP `disk_total_space` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Float, False)` reflecting PHP behaviour where `disk_total_space`
//!   returns the filesystem's total byte count on success or `false` on failure.
//! - Shares `__rt_disk_space` and its lowering with `disk_free_space`, so the two move
//!   together: a change to one of their declarations that the other does not follow would
//!   make one boxed and one raw out of a single helper.
//! - The registry pre-infers arguments before calling this hook.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "disk_total_space",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::DiskTotalSpace,
    ),
}

/// Returns `Union(Float, False)` reflecting that `disk_total_space` can return bytes or `false`.
///
/// The registry pre-infers arguments before calling this hook.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::Float, PhpType::False]))
}
