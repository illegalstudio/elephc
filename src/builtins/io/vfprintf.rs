//! Purpose:
//! Home of the PHP `vfprintf` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` calls `ensure_stream_resource` on the stream argument for validation and
//!   returns `Int`. Arguments are pre-inferred by the registry before the hook runs.
//! - `arity_error` is overridden to preserve the legacy message suffix
//!   "(stream, format, values)" that the standard derived message omits.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "vfprintf",
    area: Io,
    params: [stream: Mixed, format: Str, values: Mixed],
    arity_error: "vfprintf() takes exactly 3 arguments (stream, format, values)",
    returns: Int,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Vfprintf,
    ),
    summary: "Write a formatted string to a stream.",
    php_manual: "function.vfprintf",
}

/// Validates the stream argument is a stream resource and returns `Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Int)
}
