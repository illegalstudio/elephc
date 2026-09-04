//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_shutdown`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Applies shutdown modes through eval's stream resource table.

eval_builtin! {
    contract: "stream_socket_shutdown",
    area: Filesystem,
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

/// php-src's verbatim `ValueError` wording for a `stream_socket_shutdown()` `$mode` outside the
/// three `STREAM_SHUT_*` constants.
const STREAM_SOCKET_SHUTDOWN_BAD_MODE_MESSAGE: &str =
    "stream_socket_shutdown(): Argument #2 ($mode) must be one of STREAM_SHUT_RD, \
     STREAM_SHUT_WR, or STREAM_SHUT_RDWR";

/// Applies a socket shutdown mode to a stream resource.
pub(in crate::interpreter) fn eval_stream_socket_shutdown_result(
    stream: RuntimeCellHandle,
    mode: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = super::stream_socket_get_name::eval_socket_resource_id(stream, values)?;
    let mode = eval_int_value(mode, values)?;
    // php-src accepts only the three `STREAM_SHUT_*` constants (0, 1, 2). Every other mode used
    // to fall through to a `false` that is indistinguishable from a legal mode the kernel
    // refused.
    if !(0..=2).contains(&mode) {
        return eval_stream_value_error(
            STREAM_SOCKET_SHUTDOWN_BAD_MODE_MESSAGE,
            context,
            values,
        );
    }
    values.bool_value(
        context
            .stream_resources()
            .socket_shutdown(id, mode)
            .unwrap_or(false),
    )
}
