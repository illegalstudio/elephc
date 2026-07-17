//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_client`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Opened sockets enter eval's normal stream table.

eval_builtin! {
    name: "stream_socket_client",
    area: Filesystem,
    params: [address],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Evaluates `stream_socket_client($address)`.
pub(in crate::interpreter) fn eval_stream_socket_client_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [address] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let address = eval_expr(address, context, scope, values)?;
    eval_stream_socket_client_result(address, context, values)
}

/// Opens a connected stream from an already evaluated address argument.
pub(in crate::interpreter) fn eval_stream_socket_client_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [address] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_socket_client_result(*address, context, values)
}

/// Opens a connected TCP stream resource.
pub(in crate::interpreter) fn eval_stream_socket_client_result(
    address: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = eval_path_string(address, values)?;
    match context.stream_resources_mut().open_tcp_stream(&address) {
        Some(id) => values.resource(id),
        None => values.bool_value(false),
    }
}
