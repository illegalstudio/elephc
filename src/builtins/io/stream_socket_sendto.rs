//! Purpose:
//! Home of the PHP `stream_socket_sendto` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates arg[0] is a stream resource, then returns `Union(Int, Bool)`.
//! - Arguments are pre-inferred by the registry before the hook runs.
//! - `lower` dispatches to `io::lower_stream_socket_sendto` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
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
    lower: lower,
    summary: "Sends a message to a socket, whether it is connected or not.",
    php_manual: "function.stream-socket-sendto",
}

/// Validates arg[0] is a stream resource, then returns `Union(Int, Bool)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(cx.checker, cx.name, &cx.args[0], cx.env)?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::Int, PhpType::Bool]))
}

/// Lowers a `stream_socket_sendto` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_stream_socket_sendto(ctx, inst)
}
