//! Purpose:
//! Home of the PHP `stream_socket_accept` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates arg[0] is a stream resource and that `peer_name` (arg[2]), if provided,
//!   is a plain variable (it is written by reference).
//! - Arguments are pre-inferred by the registry before the hook runs.
//! - `lower` dispatches to `io::lower_stream_socket_accept` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "stream_socket_accept",
    area: Io,
    params: [
        socket: Mixed,
        timeout: Mixed = DefaultSpec::Null,
        ref peer_name: Mixed = DefaultSpec::Null
    ],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Accept a connection on a socket created by stream_socket_server().",
    php_manual: "function.stream-socket-accept",
}

/// Validates arg[0] is a stream resource and that `peer_name` (arg[2]) is a plain variable.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(cx.checker, cx.name, &cx.args[0], cx.env)?;
    if let Some(peer) = cx.args.get(2) {
        if !matches!(peer.kind, ExprKind::Variable(_)) {
            return Err(CompileError::new(
                peer.span,
                "stream_socket_accept() parameter $peer_name must be passed a variable",
            ));
        }
    }
    Ok(cx.checker.normalize_union_type(vec![PhpType::stream_resource(), PhpType::Bool]))
}

/// Lowers a `stream_socket_accept` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_socket_accept(ctx, inst)
}
