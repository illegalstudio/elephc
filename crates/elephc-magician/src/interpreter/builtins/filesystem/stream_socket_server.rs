//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_server`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Opened listeners enter eval's normal stream table.

eval_builtin! {
    name: "stream_socket_server",
    area: Filesystem,
    params: [address],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Evaluates `stream_socket_server($address)`.
pub(in crate::interpreter) fn eval_stream_socket_server_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [address] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let address = eval_expr(address, context, scope, values)?;
    eval_stream_socket_server_result(address, context, values)
}

/// Opens a listener from an already evaluated address argument.
pub(in crate::interpreter) fn eval_stream_socket_server_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [address] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_socket_server_result(*address, context, values)
}

/// Opens a TCP listener resource.
pub(in crate::interpreter) fn eval_stream_socket_server_result(
    address: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = eval_path_string(address, values)?;
    match context.stream_resources_mut().open_tcp_listener(&address) {
        Some(id) => values.resource(id),
        None => values.bool_value(false),
    }
}
