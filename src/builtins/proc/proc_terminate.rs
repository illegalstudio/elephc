//! Purpose:
//! Home of the PHP `proc_terminate` builtin: shared-contract binding and AOT semantics.
//!
//! Called from:
//! - `crate::builtins::registry` through the inventory submission below.
//!
//! Key details:
//! - The prelude's `__elephc_proc_terminate` sends the signal (default `SIGTERM`, 15).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "proc_terminate",
    check: check,
    semantics: super::scalar_semantics(super::bool_result),
}

/// Refuses sandboxed targets and answers `bool`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::check_spawning(cx, PhpType::Bool)
}
