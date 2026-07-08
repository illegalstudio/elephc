//! Purpose:
//! Sends and receives data through eval stream socket resources.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::stream_sockets` re-exports.
//!
//! Key details:
//! - `stream_socket_recvfrom()` preserves optional by-reference address writeback
//!   for both direct and dynamic callable dispatch.

use super::*;

/// Evaluates `stream_socket_sendto(stream, data, flags = 0, address = null)`.
pub(in crate::interpreter) fn eval_builtin_stream_socket_sendto(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let data = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        let _ = eval_expr(arg, context, scope, values)?;
    }
    eval_stream_socket_sendto_result(stream, data, context, values)
}

/// Writes bytes to a connected socket stream.
pub(in crate::interpreter) fn eval_stream_socket_sendto_result(
    stream: RuntimeCellHandle,
    data: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::fwrite::eval_fwrite_result(stream, data, context, values)
}

/// Evaluates `stream_socket_recvfrom()` over full eval call metadata.
pub(in crate::interpreter) fn eval_builtin_stream_socket_recvfrom_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    let (bound, _) = bind_evaluated_ref_builtin_args(
        &["socket", "length", "flags", "address"],
        &evaluated_args,
        false,
    )?;
    let socket = required_evaluated_ref_arg(&bound, 0)?;
    let length = required_evaluated_ref_arg(&bound, 1)?;
    let address_target = optional_evaluated_ref_arg(&bound, 3)
        .map(|arg| arg.ref_target.clone().ok_or(EvalStatus::RuntimeFatal))
        .transpose()?;
    let (result, address) =
        eval_stream_socket_recvfrom_with_address_result(socket.value, length.value, context, values)?;
    eval_write_socket_output_ref_target(address_target.as_ref(), address, context, values)?;
    Ok(result)
}

/// Reads bytes from a connected socket stream.
pub(in crate::interpreter) fn eval_stream_socket_recvfrom_result(
    stream: RuntimeCellHandle,
    length: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::fread::eval_fread_result(stream, length, context, values)
}

/// Reads bytes from a connected socket stream and returns the tracked remote endpoint name.
pub(in crate::interpreter) fn eval_stream_socket_recvfrom_with_address_result(
    stream: RuntimeCellHandle,
    length: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<String>), EvalStatus> {
    let id = eval_socket_resource_id(stream, values)?;
    let address = context.stream_resources().socket_name(id, true);
    let result = super::super::fread::eval_fread_result(stream, length, context, values)?;
    Ok((result, address))
}
