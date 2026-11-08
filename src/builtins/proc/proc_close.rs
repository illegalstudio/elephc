//! Purpose:
//! Home of the PHP `proc_close` builtin: shared-contract binding and AOT semantics.
//!
//! Called from:
//! - `crate::builtins::registry` through the inventory submission below.
//!
//! Key details:
//! - The prelude's `__elephc_proc_close` closes the pipes, waits, and marks the
//!   resource closed; php-src's TypeErrors for a non-process argument live there too.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "proc_close",
    check: check,
    semantics: super::scalar_semantics(super::int_result),
}

/// Refuses sandboxed targets and answers `int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::check_spawning(cx, PhpType::Int)
}
