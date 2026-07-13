//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_recvfrom`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//! - `crate::interpreter::expressions::eval_call()` for address writeback.
//!
//! Key details:
//! - Reads delegate to `fread`, while optional address writeback uses tracked
//!   remote endpoint metadata from eval's stream table.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_socket_recvfrom",
    area: Filesystem,
    params: [
        socket,
        length,
        flags = EvalBuiltinDefaultValue::Int(0),
        address: by_ref = EvalBuiltinDefaultValue::String("")
    ],
    by_ref: [address],
    direct: none,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates a positional `stream_socket_recvfrom()` call without writable address output.
pub(in crate::interpreter) fn eval_stream_socket_recvfrom_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let length = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        let _ = eval_expr(arg, context, scope, values)?;
    }
    if args.len() >= 4 {
        values.warning(
            "stream_socket_recvfrom(): Argument #4 ($address) must be passed by reference, value given",
        )?;
    }
    eval_stream_socket_recvfrom_result(stream, length, context, values)
}

/// Reads bytes from already evaluated socket receive arguments.
pub(in crate::interpreter) fn eval_stream_socket_recvfrom_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if evaluated_args.len() >= 4 {
        values.warning(
            "stream_socket_recvfrom(): Argument #4 ($address) must be passed by reference, value given",
        )?;
    }
    eval_stream_socket_recvfrom_result(evaluated_args[0], evaluated_args[1], context, values)
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
    let (result, address) = eval_stream_socket_recvfrom_with_address_result(
        socket.value,
        length.value,
        context,
        values,
    )?;
    super::fsockopen::eval_write_socket_output_ref_target(
        address_target.as_ref(),
        address,
        context,
        values,
    )?;
    Ok(result)
}

/// Reads bytes from a connected socket stream.
pub(in crate::interpreter) fn eval_stream_socket_recvfrom_result(
    stream: RuntimeCellHandle,
    length: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::fread::eval_fread_result(stream, length, context, values)
}

/// Reads bytes from a connected socket stream and returns the tracked remote endpoint name.
pub(in crate::interpreter) fn eval_stream_socket_recvfrom_with_address_result(
    stream: RuntimeCellHandle,
    length: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<String>), EvalStatus> {
    let id = super::stream_socket_get_name::eval_socket_resource_id(stream, values)?;
    let address = context.stream_resources().socket_name(id, true);
    let result = super::fread::eval_fread_result(stream, length, context, values)?;
    Ok((result, address))
}
