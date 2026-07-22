//! Purpose:
//! Home of the PHP `stream_get_transports` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Array(Str)`, which is not scalar-expressible, so `returns: Mixed` is
//!   used and the hook overrides the return type. The hook takes no arguments.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "stream_get_transports",
    area: Io,
    params: [],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamGetTransports,
    ),
    summary: "Retrieves list of registered socket transports.",
    php_manual: "function.stream-get-transports",
}

/// Returns `Array(Str)` as the precise return type for `stream_get_transports`.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}
