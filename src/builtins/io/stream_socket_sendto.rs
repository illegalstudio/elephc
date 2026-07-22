//! Purpose:
//! Home of the PHP `stream_socket_sendto` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates arg[0] is a stream resource, then returns `Union(Int, Bool)`.
//! - Arguments are pre-inferred by the registry before the hook runs.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "stream_socket_sendto",
    area: Io,
    params: [
        socket: Mixed,
        data: Str,
        flags: Int = DefaultSpec::Int(0),
        address: Str = DefaultSpec::Str("")
    ],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSocketSendto,
    ),
    summary: "Sends a message to a socket, whether it is connected or not.",
    php_manual: "function.stream-socket-sendto",
}

/// Validates arg[0] is a stream resource, then returns `Union(Int, Bool)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(cx.checker, cx.name, &cx.args[0], cx.env)?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::Int, PhpType::False]))
}
