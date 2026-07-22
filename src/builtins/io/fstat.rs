//! Purpose:
//! Home of the PHP `fstat` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the `stream` argument is a stream resource via
//!   `ensure_stream_resource`, then returns `assoc-array<mixed, int>|bool` via
//!   `stat_result_type`. PHP's `fstat` returns the stat buffer array on success or
//!   `false` on failure.
//! - `ensure_stream_resource` is kept in `common.rs` (not moved) because
//!   `streams.rs` also uses it; it is widened to `pub(crate)` for access here.
//! - The registry pre-infers arguments before calling this hook (idempotent with
//!   the infer call inside `ensure_stream_resource`).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "fstat",
    area: Io,
    params: [stream: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fstat,
    ),
    summary: "Gets information about a file using an open file pointer.",
    php_manual: "function.fstat",
}

/// Validates `stream` is a stream resource and returns `assoc-array<mixed, int>|bool`.
///
/// Calls `ensure_stream_resource` to emit a type error if the argument is not a
/// compatible stream type, then returns the stat result type via `stat_result_type`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(crate::builtins::io::stat_support::stat_result_type(cx.checker))
}
