//! Purpose:
//! Declarative eval registry entry for `ftruncate`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream truncate helper.

eval_builtin! {
    name: "ftruncate",
    area: Filesystem,
    params: [stream, size],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `ftruncate` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_ftruncate_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_ftruncate(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `ftruncate` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_ftruncate_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, size] => eval_ftruncate_result(*stream, *size, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `ftruncate($stream, $size)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_ftruncate(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, size] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let size = eval_expr(size, context, scope, values)?;
    eval_ftruncate_result(stream, size, context, values)
}

/// Truncates a materialized stream resource.
pub(in crate::interpreter) fn eval_ftruncate_result(
    stream: RuntimeCellHandle,
    size: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let size = eval_int_value(size, values)?;
    let Ok(size) = u64::try_from(size) else {
        return values.bool_value(false);
    };
    if let Some(result) = eval_user_wrapper_ftruncate_result(id, size, context, values)? {
        return Ok(result);
    }
    values.bool_value(context.stream_resources_mut().truncate(id, size))
}
