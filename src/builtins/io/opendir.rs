//! Purpose:
//! Home of the PHP `opendir` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(stream_resource, False)` to reflect PHP's false-on-failure
//!   pattern. The `directory` argument is a path string, not a resource; it is
//!   pre-inferred by the registry and no resource validation is performed.
//! - The shared catalog declares `resource|false`; the checker refines its native stream kind.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "opendir",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Opendir,
    ),
}

/// Returns `Union(stream_resource, False)` for the directory open result.
///
/// The `directory` argument is a path string, not a stream resource; no resource
/// validation is performed here. The common registry path pre-infers the argument.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::stream_resource(),
        PhpType::False,
    ]))
}
