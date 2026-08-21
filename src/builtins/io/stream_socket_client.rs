//! Purpose:
//! Home of the PHP `stream_socket_client` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(stream_resource, Bool)` reflecting PHP's false-on-failure return.
//! - `returns: Mixed` is used because the union cannot be expressed through the scalar field.
//! - `error_code` and `error_message` are declared `ref(Int)` / `ref(Str)`: the runtime writes
//!   them on failure, so the caller may pass them undeclared as PHP's own idiom does.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "stream_socket_client",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSocketClient,
    ),
}

/// Returns PHP's `resource|false` result. The by-reference outputs need no check here: their
/// `ref(T)` declarations carry the rule.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::stream_resource(), PhpType::False]))
}