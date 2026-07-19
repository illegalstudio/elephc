//! Purpose:
//! Home of the PHP `stream_socket_server` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(stream_resource, Bool)` reflecting PHP's false-on-failure return.
//! - `returns: Mixed` is used because the union cannot be expressed through the scalar field.
//! - `lower` dispatches to `io::lower_stream_socket_server` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "stream_socket_server",
    area: Io,
    params: [address: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Create an Internet or Unix domain server socket.",
    php_manual: "function.stream-socket-server",
}

/// Returns `Union(stream_resource, Bool)` reflecting PHP's false-on-failure return.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::stream_resource(), PhpType::False]))
}

/// Lowers a `stream_socket_server` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_stream_socket_server(ctx, inst)
}
