//! Purpose:
//! Home of the PHP `popen` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(stream_resource, False)` to reflect PHP's false-on-failure
//!   pattern. The arguments are a command string and mode string, not resources; they
//!   are pre-inferred by the registry and no resource validation is performed.
//! - The shared catalog declares `resource|false`; the checker refines its native stream kind.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "popen",
    check: check,
    semantics: crate::builtins::semantics::host_only_runtime_fn_semantics(
        crate::ir::RuntimeFnId::Popen,
    ),
}

/// Returns `Union(stream_resource, False)` for the pipe open result.
///
/// The arguments are command and mode strings, not stream resources; no resource
/// validation is performed here. The common registry path pre-infers the arguments.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::builtins::spec::reject_if_process_spawn_forbidden(cx)?;
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::stream_resource(),
        PhpType::False,
    ]))
}
