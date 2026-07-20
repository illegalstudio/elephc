//! Purpose:
//! Eval registry entry and implementation for `printf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - `printf()` reuses `sprintf` byte formatting, echoes the result, and returns
//!   the emitted byte count.

use super::super::super::*;

eval_builtin! {
    name: "printf",
    area: Formatting,
    params: [format],
    variadic: values,
    direct: Printf,
    values: Printf,
}

/// Evaluates direct positional `printf()` calls in source order.
pub(in crate::interpreter) fn eval_builtin_printf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_printf_result(&evaluated_args, values)
}

/// Formats `printf()` arguments, echoes the result, and returns its byte count.
pub(in crate::interpreter) fn eval_printf_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((format, format_args)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    let output = super::sprintf::eval_sprintf_bytes(&format, format_args, values)?;
    let len = i64::try_from(output.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let output = values.string_bytes_value(&output)?;
    values.echo(output)?;
    values.int(len)
}
