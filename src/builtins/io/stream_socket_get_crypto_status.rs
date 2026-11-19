//! Purpose:
//! Home of the PHP 8.6 `stream_socket_get_crypto_status` builtin: its single-source registry
//! declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates arg[0] is a stream resource and returns the integer status.
//! - The result is one of `STREAM_CRYPTO_STATUS_NONE` / `WANT_READ` / `WANT_WRITE`; elephc's
//!   blocking TLS handshake can only report `NONE`, and a stream without an SSL layer warns
//!   and also reports `NONE`, matching php-src.
//! - Arguments are pre-inferred by the registry before the hook runs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "stream_socket_get_crypto_status",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSocketGetCryptoStatus,
    ),
}

/// Validates arg[0] is a stream resource and returns the integer crypto status.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Int)
}
