//! Purpose:
//! Home of the PHP `stream_socket_shutdown` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates arg[0] is a stream resource, then returns `Bool`.
//! - Arguments are pre-inferred by the registry before the hook runs.
//! - `lower` dispatches to `io::lower_stream_socket_shutdown` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "stream_socket_shutdown",
    area: Io,
    params: [stream: Mixed, mode: Int],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Shutdown a full-duplex connection.",
    php_manual: "function.stream-socket-shutdown",
}

/// Validates arg[0] is a stream resource, then returns `Bool`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(cx.checker, cx.name, &cx.args[0], cx.env)?;
    Ok(PhpType::Bool)
}

/// Lowers a `stream_socket_shutdown` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_stream_socket_shutdown(ctx, inst)
}
