//! Purpose:
//! Home of the PHP `stream_socket_enable_crypto` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates arg[0] is a stream resource and requires the `elephc_tls` library.
//! - Arguments are pre-inferred by the registry before the hook runs.
//! - `lower` dispatches to `io::lower_stream_socket_enable_crypto` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "stream_socket_enable_crypto",
    area: Io,
    params: [
        stream: Mixed,
        enable: Bool,
        crypto_method: Mixed = DefaultSpec::Null,
        session_stream: Mixed = DefaultSpec::Null
    ],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Turns encryption on/off on an already connected socket.",
    php_manual: "function.stream-socket-enable-crypto",
}

/// Validates arg[0] is a stream resource, links the TLS library, and returns `Bool`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(cx.checker, cx.name, &cx.args[0], cx.env)?;
    cx.checker.require_builtin_library("elephc_tls");
    Ok(PhpType::Bool)
}

/// Lowers a `stream_socket_enable_crypto` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_stream_socket_enable_crypto(ctx, inst)
}
