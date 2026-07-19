//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_shutdown`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Applies shutdown modes through eval's stream resource table.

eval_builtin! {
    name: "stream_socket_shutdown",
    area: Filesystem,
    params: [stream, mode],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_socket_shutdown($stream, $mode)`.
pub(in crate::interpreter) fn eval_stream_socket_shutdown_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, mode] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let mode = eval_expr(mode, context, scope, values)?;
    eval_stream_socket_shutdown_result(stream, mode, context, values)
}

/// Shuts down an already evaluated socket stream argument.
pub(in crate::interpreter) fn eval_stream_socket_shutdown_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, mode] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_socket_shutdown_result(*stream, *mode, context, values)
}

/// Applies a socket shutdown mode to a stream resource.
pub(in crate::interpreter) fn eval_stream_socket_shutdown_result(
    stream: RuntimeCellHandle,
    mode: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = super::stream_socket_get_name::eval_socket_resource_id(stream, values)?;
    let mode = eval_int_value(mode, values)?;
    values.bool_value(
        context
            .stream_resources()
            .socket_shutdown(id, mode)
            .unwrap_or(false),
    )
}
