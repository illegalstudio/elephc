//! Purpose:
//! Declarative eval registry entry for `stream_copy_to_stream`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream copy helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_copy_to_stream",
    area: Filesystem,
    params: [
        from,
        to,
        length = EvalBuiltinDefaultValue::Null,
        offset = EvalBuiltinDefaultValue::Int(-1)
    ],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `stream_copy_to_stream` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_copy_to_stream_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_stream_copy_to_stream(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_copy_to_stream` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_copy_to_stream_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [from, to] => eval_stream_copy_to_stream_result(*from, *to, None, None, context, values),
        [from, to, length] => eval_stream_copy_to_stream_result(*from, *to, Some(*length), None, context, values),
        [from, to, length, offset] => eval_stream_copy_to_stream_result(*from, *to, Some(*length), Some(*offset), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `stream_copy_to_stream($from, $to, $length = null, $offset = -1)`.
pub(in crate::interpreter) fn eval_builtin_stream_copy_to_stream(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let from = eval_expr(&args[0], context, scope, values)?;
    let to = eval_expr(&args[1], context, scope, values)?;
    let length = match args.get(2) {
        Some(length) => Some(eval_expr(length, context, scope, values)?),
        None => None,
    };
    let offset = match args.get(3) {
        Some(offset) => Some(eval_expr(offset, context, scope, values)?),
        None => None,
    };
    eval_stream_copy_to_stream_result(from, to, length, offset, context, values)
}

/// Copies bytes between two materialized stream resources.
pub(in crate::interpreter) fn eval_stream_copy_to_stream_result(
    from: RuntimeCellHandle,
    to: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    offset: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let from = eval_stream_resource_id(from, values)?;
    let to = eval_stream_resource_id(to, values)?;
    let length = eval_optional_stream_length(length, values)?;
    let offset = eval_optional_stream_offset(offset, values)?;
    if let Some(result) =
        eval_user_wrapper_stream_copy_to_stream_result(from, to, length, offset, context, values)?
    {
        return Ok(result);
    }
    match context
        .stream_resources_mut()
        .copy_to_stream(from, to, length, offset)
    {
        Some(written) => values.int(i64::try_from(written).map_err(|_| EvalStatus::RuntimeFatal)?),
        None => values.bool_value(false),
    }
}
