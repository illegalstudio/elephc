//! Purpose:
//! Home of the PHP `stream_socket_enable_crypto` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates arg[0] is a stream resource and requires the `elephc_tls` library.
//! - PHP returns `true` on completion, integer `0` while a nonblocking
//!   handshake needs more I/O, and `false` on failure.
//! - Arguments are pre-inferred by the registry before the hook runs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "stream_socket_enable_crypto",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSocketEnableCrypto,
    ),
}

/// Validates arg[0] is a stream resource, links the TLS library, and returns PHP's
/// `true|0|false` handshake result.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(cx.checker, cx.name, &cx.args[0], cx.env)?;
    Ok(PhpType::Union(vec![PhpType::Bool, PhpType::Int]))
}
