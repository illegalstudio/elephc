//! Purpose:
//! Home of the PHP `opendir` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(stream_resource, Bool)` to reflect PHP's false-on-failure
//!   pattern. The `directory` argument is a path string, not a resource — it is
//!   pre-inferred by the registry and no resource validation is performed.
//! - `returns: Mixed` is used because the union involves a resource type that the
//!   scalar `returns:` field cannot express.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "opendir",
    area: Io,
    params: [directory: Str],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Opendir,
    ),
    summary: "Open directory handle.",
    php_manual: "function.opendir",
}

/// Returns `Union(stream_resource, Bool)` for the directory open result.
///
/// The `directory` argument is a path string, not a stream resource; no resource
/// validation is performed here. The common registry path pre-infers the argument.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::stream_resource(),
        PhpType::Bool,
    ]))
}
