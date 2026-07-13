//! Purpose:
//! Declarative eval registry entry for `fwrite`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream write helper.

eval_builtin! {
    name: "fwrite",
    area: Filesystem,
    params: [stream, data],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fwrite` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fwrite_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_fwrite(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fwrite` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fwrite_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, data] => eval_fwrite_result(*stream, *data, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `fwrite($stream, $data)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fwrite(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, data] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let data = eval_expr(data, context, scope, values)?;
    eval_fwrite_result(stream, data, context, values)
}

/// Writes bytes to a materialized stream resource.
pub(in crate::interpreter) fn eval_fwrite_result(
    stream: RuntimeCellHandle,
    data: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let data = values.string_bytes(data)?;
    if let Some(result) = eval_user_wrapper_fwrite_result(id, &data, context, values)? {
        return Ok(result);
    }
    match context.stream_resources_mut().write(id, &data) {
        Some(written) => values.int(i64::try_from(written).map_err(|_| EvalStatus::RuntimeFatal)?),
        None => values.bool_value(false),
    }
}
