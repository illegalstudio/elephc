//! Purpose:
//! Declarative eval registry entry for `fread`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream read helper.

eval_builtin! {
    name: "fread",
    area: Filesystem,
    params: [stream, length],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fread` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fread_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_fread(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fread` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fread_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, length] => eval_fread_result(*stream, *length, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `fread($stream, $length)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fread(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, length] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let length = eval_expr(length, context, scope, values)?;
    eval_fread_result(stream, length, context, values)
}

/// Reads bytes from a materialized stream resource.
pub(in crate::interpreter) fn eval_fread_result(
    stream: RuntimeCellHandle,
    length: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let length = eval_nonnegative_usize(length, values)?;
    if let Some(result) = eval_user_wrapper_fread_result(id, length, context, values)? {
        return Ok(result);
    }
    match context.stream_resources_mut().read(id, length) {
        Some(bytes) => values.string_bytes_value(&bytes),
        None => values.bool_value(false),
    }
}
