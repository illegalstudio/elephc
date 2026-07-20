//! Purpose:
//! Declarative eval registry entry for `fprintf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Variadic values are formatted by the existing printf-family helper.

eval_builtin! {
    name: "fprintf",
    area: Filesystem,
    params: [stream, format],
    variadic: values,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fprintf` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fprintf_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_fprintf(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fprintf` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fprintf_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((stream, rest)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let Some((format, format_args)) = rest.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_fprintf_result(*stream, *format, format_args, context, values)
}

/// Evaluates PHP `fprintf($stream, $format, ...$values)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fprintf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let format = eval_expr(&args[1], context, scope, values)?;
    let mut format_args = Vec::with_capacity(args.len().saturating_sub(2));
    for arg in &args[2..] {
        format_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_fprintf_result(stream, format, &format_args, context, values)
}

/// Formats and writes `fprintf()` arguments to a materialized stream resource.
pub(in crate::interpreter) fn eval_fprintf_result(
    stream: RuntimeCellHandle,
    format: RuntimeCellHandle,
    format_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let format = values.string_bytes(format)?;
    let output = eval_sprintf_bytes(&format, format_args, values)?;
    match context.stream_resources_mut().write(id, &output) {
        Some(written) => values.int(i64::try_from(written).map_err(|_| EvalStatus::RuntimeFatal)?),
        None => values.bool_value(false),
    }
}
