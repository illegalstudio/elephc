//! Purpose:
//! Home of the PHP `stream_socket_recvfrom` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates arg[0] is a stream resource and that `address` (arg[3]), if provided,
//!   is a plain string variable (it is written by reference). The double-infer of arg[3]
//!   matches the legacy behavior.
//! - Arguments are pre-inferred by the registry before the hook runs.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "stream_socket_recvfrom",
    area: Io,
    params: [
        socket: Mixed,
        length: Int,
        flags: Int = DefaultSpec::Int(0),
        ref address: Str = DefaultSpec::Str("")
    ],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamSocketRecvfrom,
    ),
    summary: "Receives data from a socket, connected or not.",
    php_manual: "function.stream-socket-recvfrom",
}

/// Validates arg[0] is a stream resource and that `address` (arg[3]) is a plain string variable.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(cx.checker, cx.name, &cx.args[0], cx.env)?;
    if cx.args.len() == 4 {
        let addr = &cx.args[3];
        if !matches!(addr.kind, ExprKind::Variable(_)) {
            return Err(CompileError::new(
                addr.span,
                "stream_socket_recvfrom() parameter $address must be passed a variable",
            ));
        }
        let ty = cx.checker.infer_type(addr, cx.env)?;
        if ty != PhpType::Str {
            return Err(CompileError::new(
                addr.span,
                "stream_socket_recvfrom() parameter $address must be a string",
            ));
        }
    }
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
