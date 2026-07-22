//! Purpose:
//! Home of the PHP `popen` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(stream_resource, Bool)` to reflect PHP's false-on-failure
//!   pattern. The arguments are a command string and mode string, not resources — they
//!   are pre-inferred by the registry and no resource validation is performed.
//! - `returns: Mixed` is used because the union involves a resource type that the
//!   scalar `returns:` field cannot express.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "popen",
    area: Io,
    params: [command: Str, mode: Str],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Popen,
    ),
    summary: "Opens process file pointer.",
    php_manual: "function.popen",
}

/// Returns `Union(stream_resource, Bool)` for the pipe open result.
///
/// The arguments are command and mode strings, not stream resources; no resource
/// validation is performed here. The common registry path pre-infers the arguments.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::stream_resource(),
        PhpType::Bool,
    ]))
}
