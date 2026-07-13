//! Purpose:
//! Declarative eval registry entry and implementation for `stream_socket_sendto`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Connected socket writes delegate to the same eval stream write path as `fwrite`.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_socket_sendto",
    area: Filesystem,
    params: [
        socket,
        data,
        flags = EvalBuiltinDefaultValue::Int(0),
        address = EvalBuiltinDefaultValue::String("")
    ],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_socket_sendto($socket, $data, $flags = 0, $address = "")`.
pub(in crate::interpreter) fn eval_stream_socket_sendto_declared_call(
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

/// Writes bytes from already evaluated socket send arguments.
pub(in crate::interpreter) fn eval_stream_socket_sendto_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_stream_socket_sendto_result(evaluated_args[0], evaluated_args[1], context, values)
}

/// Writes bytes to a connected socket stream.
pub(in crate::interpreter) fn eval_stream_socket_sendto_result(
    stream: RuntimeCellHandle,
    data: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::fwrite::eval_fwrite_result(stream, data, context, values)
}
