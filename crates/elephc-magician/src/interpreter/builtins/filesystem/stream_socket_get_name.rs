//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_get_name`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Socket resources use eval's one-based displayed resource ids and zero-based
//!   internal stream table ids.

eval_builtin! {
    name: "stream_socket_get_name",
    area: Filesystem,
    params: [socket, remote],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_socket_get_name($socket, $remote)`.
pub(in crate::interpreter) fn eval_stream_socket_get_name_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [socket, remote] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let socket = eval_expr(socket, context, scope, values)?;
    let remote = eval_expr(remote, context, scope, values)?;
    eval_stream_socket_get_name_result(socket, remote, context, values)
}

/// Returns a socket name for already evaluated socket and remote arguments.
pub(in crate::interpreter) fn eval_stream_socket_get_name_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [socket, remote] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_socket_get_name_result(*socket, *remote, context, values)
}

/// Returns a tracked local or remote socket endpoint name.
pub(in crate::interpreter) fn eval_stream_socket_get_name_result(
    socket: RuntimeCellHandle,
    remote: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_socket_resource_id(socket, values)?;
    let remote = values.truthy(remote)?;
    match context.stream_resources().socket_name(id, remote) {
        Some(name) => values.string(&name),
        None => values.bool_value(false),
    }
}

/// Converts a runtime resource cell into eval's zero-based socket id.
pub(in crate::interpreter) fn eval_socket_resource_id(
    resource: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(resource)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(resource, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}
