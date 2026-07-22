//! Purpose:
//! Home of the PHP `fscanf` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` calls `ensure_stream_resource` on the stream argument for validation and
//!   returns `Array<Str>` reflecting the 2-argument form that returns matched fields.
//!   `returns: Mixed` is used because `Array<Str>` cannot be expressed through the
//!   scalar `returns:` field. Arguments are pre-inferred by the registry before the
//!   hook runs.
//! - The variadic `vars` parameter is accepted but the by-ref output form is not yet
//!   supported (mirroring `sscanf()`).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "fscanf",
    area: Io,
    params: [stream: Mixed, format: Str],
    variadic: "vars",
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fscanf,
    ),
    summary: "Parses input from a file according to a format.",
    php_manual: "function.fscanf",
}

/// Validates the stream argument and returns `Array<Str>` for the matched-fields result.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}
