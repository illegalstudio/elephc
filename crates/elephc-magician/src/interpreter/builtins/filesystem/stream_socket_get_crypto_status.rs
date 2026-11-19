//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_get_crypto_status`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - eval does not manage TLS handshake state, so a tracked stream reports
//!   `STREAM_CRYPTO_STATUS_NONE`, matching the AOT runtime's blocking handshake.
//! - The argument is validated through the shared socket-resource decoder so a non-resource
//!   fails the same way as the other `stream_socket_*` builtins.

eval_builtin! {
    contract: "stream_socket_get_crypto_status",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_socket_get_crypto_status($stream)`.
pub(in crate::interpreter) fn eval_stream_socket_get_crypto_status_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    eval_stream_socket_get_crypto_status_result(stream, values)
}

/// Returns the crypto status for an already evaluated stream argument.
pub(in crate::interpreter) fn eval_stream_socket_get_crypto_status_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_socket_get_crypto_status_result(*stream, values)
}

/// Reports `STREAM_CRYPTO_STATUS_NONE`; eval has no pending TLS operation.
pub(in crate::interpreter) fn eval_stream_socket_get_crypto_status_result(
    stream: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let _ = super::stream_socket_get_name::eval_socket_resource_id(stream, values)?;
    values.int(0)
}
