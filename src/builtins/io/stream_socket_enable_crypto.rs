//! Purpose:
//! Home of the PHP `stream_socket_enable_crypto` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates arg[0] is a stream resource and requires the `elephc_tls` library.
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

/// Validates arg[0] is a stream resource, links the TLS library, and returns `Mixed`.
///
/// php-src declares `int|bool`, not `bool`: a NON-BLOCKING socket whose TLS handshake has not
/// finished yet answers `0` and expects the caller to retry. Returning `Bool` here overrode the
/// shared contract's wider `Mixed` and made `if ($r === 0)` unreachable code for the checker,
/// so the two authorities disagreed on the same builtin.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(cx.checker, cx.name, &cx.args[0], cx.env)?;
    Ok(PhpType::Mixed)
}
