//! Purpose:
//! Home of the PHP `stream_filter_register` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the class argument names a declared class and returns `Bool`.
//! - Arguments are pre-inferred by the registry before the hook runs; the hook does NOT
//!   re-infer them.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "stream_filter_register",
    area: Io,
    params: [filter_name: Str, class: Str],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamFilterRegister,
    ),
    summary: "Registers a user-defined stream filter.",
    php_manual: "function.stream-filter-register",
}

/// Validates the class argument names a declared class and returns `Bool`.
///
/// Arguments are pre-inferred by the registry; this hook validates the class
/// registration using the shared `validate_registered_stream_class` helper.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::builtins::io::stream_support::validate_registered_stream_class(
        cx.checker,
        cx.name,
        &cx.args[1],
        cx.span,
    )?;
    Ok(PhpType::Bool)
}
