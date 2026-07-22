//! Purpose:
//! Home of the PHP `stream_context_set_default` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `PhpType::stream_resource()` which is not scalar-expressible, so
//!   `returns: Mixed` is used and the hook overrides the return type.
//! - Arguments are pre-inferred by the registry before the hook runs; the hook does NOT
//!   re-infer them.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "stream_context_set_default",
    area: Io,
    params: [options: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamContextSetDefault,
    ),
    summary: "Sets the default stream context.",
    php_manual: "function.stream-context-set-default",
}

/// Returns `stream_resource()` as the precise return type for `stream_context_set_default`.
///
/// Arguments are pre-inferred by the registry; this hook only refines the return type
/// beyond what the scalar `returns: Mixed` field can express.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::stream_resource())
}
