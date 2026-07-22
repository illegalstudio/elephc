//! Purpose:
//! Home of the PHP `fgetcsv` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the `stream` argument is a stream resource and returns `Array<Str>`.
//! - `returns: Mixed` is used because the array type cannot be expressed through the
//!   scalar `returns:` field. Arguments are pre-inferred by the registry before the hook runs.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "fgetcsv",
    area: Io,
    params: [stream: Mixed, length: Int = DefaultSpec::Null, separator: Str = DefaultSpec::Str(",")],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fgetcsv,
    ),
    summary: "Gets line from file pointer and parse for CSV fields.",
    php_manual: "function.fgetcsv",
}

/// Validates the stream argument is a stream resource and returns `Array<Str>`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}
