//! Purpose:
//! Declarative eval registry entry for `vfprintf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the vprintf-family stream write helper.

eval_builtin! {
    name: "vfprintf",
    area: Filesystem,
    params: [stream, format, values],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `vfprintf` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_vfprintf_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_vfprintf(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `vfprintf` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_vfprintf_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, format, array] => eval_vfprintf_result(*stream, *format, *array, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `vfprintf($stream, $format, $values)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_vfprintf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, format, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let format = eval_expr(format, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    eval_vfprintf_result(stream, format, array, context, values)
}

/// Formats and writes `vfprintf()` array arguments to a materialized stream resource.
pub(in crate::interpreter) fn eval_vfprintf_result(
    stream: RuntimeCellHandle,
    format: RuntimeCellHandle,
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let format_args = eval_sprintf_argument_array_values(array, values)?;
    super::fprintf::eval_fprintf_result(stream, format, &format_args, context, values)
}
