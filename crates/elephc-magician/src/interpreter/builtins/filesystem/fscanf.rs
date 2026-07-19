//! Purpose:
//! Declarative eval registry entry for `fscanf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The current eval implementation returns parsed values and ignores output vars.

eval_builtin! {
    name: "fscanf",
    area: Filesystem,
    params: [stream, format],
    variadic: vars,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `fscanf` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fscanf_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_fscanf(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fscanf` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fscanf_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_fscanf_result(evaluated_args[0], evaluated_args[1], context, values)
}

/// Evaluates PHP `fscanf($stream, $format, ...$vars)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_fscanf(
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
    for arg in &args[2..] {
        eval_expr(arg, context, scope, values)?;
    }
    eval_fscanf_result(stream, format, context, values)
}

/// Reads one line from a stream and scans it with the eval `sscanf()` subset.
pub(in crate::interpreter) fn eval_fscanf_result(
    stream: RuntimeCellHandle,
    format: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let Some(line) = context
        .stream_resources_mut()
        .read_line(id, usize::MAX, None, true, true)
    else {
        return values.bool_value(false);
    };
    let input = values.string_bytes_value(&line)?;
    eval_sscanf_result(input, format, values)
}
