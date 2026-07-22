//! Purpose:
//! Home of the PHP `fflush` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the `stream` argument is a stream resource and returns `Bool`.
//! - Arguments are pre-inferred by the registry before the hook runs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "fflush",
    area: Io,
    params: [stream: Mixed],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fflush,
    ),
    summary: "Flushes the output to a file.",
    php_manual: "function.fflush",
}

/// Validates the stream argument is a stream resource and returns `Bool`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Bool)
}
