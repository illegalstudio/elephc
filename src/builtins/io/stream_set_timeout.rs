//! Purpose:
//! Home of the PHP `stream_set_timeout` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the first argument is a stream resource before returning `Bool`.
//! - `microseconds` is optional (defaults to 0). Arguments are pre-inferred by the registry.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "stream_set_timeout",
    area: Io,
    params: [stream: Mixed, seconds: Int, microseconds: Int = DefaultSpec::Int(0)],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSetTimeout,
    ),
    summary: "Sets timeout period on a stream.",
    php_manual: "function.stream-set-timeout",
}

/// Validates the stream resource argument and returns `Bool`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Bool)
}
