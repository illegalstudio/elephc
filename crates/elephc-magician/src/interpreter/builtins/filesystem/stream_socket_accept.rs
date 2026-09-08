//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_accept`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//! - `crate::interpreter::expressions::eval_call()` for peer-name writeback.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference peer-name path.

eval_builtin! {
    contract: "stream_socket_accept",
    area: Filesystem,
    direct: none,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates a positional `stream_socket_accept()` call without writable peer output.
pub(in crate::interpreter) fn eval_stream_socket_accept_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let args = args.iter().collect::<Vec<_>>();
    with_eval_operands(&args, context, scope, values, |arguments, context, _, values| {
        eval_stream_socket_accept_declared_values_result(arguments, context, values)
    })
}

/// Accepts a socket from already evaluated by-value arguments.
pub(in crate::interpreter) fn eval_stream_socket_accept_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=3).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if evaluated_args.len() >= 3 {
        values.warning(
            "stream_socket_accept(): Argument #3 ($peer_name) must be passed by reference, value given",
        )?;
    }
    eval_stream_socket_accept_result(evaluated_args[0], context, values)
}

/// Evaluates `stream_socket_accept()` over full eval call metadata.
pub(in crate::interpreter) fn eval_builtin_stream_socket_accept_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated_args, context, _, values| {
        let (bound, _) =
            bind_evaluated_ref_builtin_args(&["socket", "timeout", "peer_name"], &evaluated_args, false)?;
        let socket = required_evaluated_ref_arg(&bound, 0)?;
        let peer_name_target = optional_evaluated_ref_arg(&bound, 2)
            .map(|arg| arg.ref_target.clone().ok_or(EvalStatus::RuntimeFatal))
            .transpose()?;
        let (result, peer_name) =
            eval_stream_socket_accept_with_peer_result(socket.value, context, values)?;
        super::fsockopen::eval_write_socket_output_ref_target(
            peer_name_target.as_ref(),
            peer_name,
            context,
            values,
        )?;
        Ok(result)
    })
}

/// Accepts one pending TCP connection from a listener resource.
pub(in crate::interpreter) fn eval_stream_socket_accept_result(
    socket: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = super::stream_socket_get_name::eval_socket_resource_id(socket, values)?;
    match context.stream_resources_mut().accept_tcp(id) {
        Some(id) => values.resource(id),
        None => values.bool_value(false),
    }
}

/// Accepts one TCP connection and returns the accepted resource plus peer endpoint name.
pub(in crate::interpreter) fn eval_stream_socket_accept_with_peer_result(
    socket: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<String>), EvalStatus> {
    let id = super::stream_socket_get_name::eval_socket_resource_id(socket, values)?;
    let Some(accepted_id) = context.stream_resources_mut().accept_tcp(id) else {
        return values.bool_value(false).map(|result| (result, None));
    };
    let peer_name = context.stream_resources().socket_name(accepted_id, true);
    let result = values.resource(accepted_id)?;
    Ok((result, peer_name))
}
