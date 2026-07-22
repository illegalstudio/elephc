//! Purpose:
//! Home of the PHP `stream_get_line` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the first argument is a stream resource before returning `Str`.
//! - `ending` is optional (defaults to empty string). Arguments are pre-inferred by the registry.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "stream_get_line",
    area: Io,
    params: [stream: Mixed, length: Int, ending: Str = DefaultSpec::Str("")],
    returns: Str,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamGetLine,
    ),
    summary: "Gets line from stream resource up to a given delimiter.",
    php_manual: "function.stream-get-line",
}

/// Validates the stream resource argument and returns `Str`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Str)
}
