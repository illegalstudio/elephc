//! Purpose:
//! Eval registry entry and implementation for `vprintf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - `vprintf()` reuses `sprintf` byte formatting and `vsprintf` argument-array
//!   expansion, then echoes the result and returns its byte count.

use super::super::super::*;

eval_builtin! {
    contract: "vprintf",
    area: Formatting,
    direct: Vprintf,
    values: Vprintf,
}

/// Evaluates direct positional `vprintf()` calls in source order.
pub(in crate::interpreter) fn eval_builtin_vprintf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_vprintf_result(&evaluated_args, context, values)
}

/// Formats `vprintf()` array arguments, echoes the result, and returns its byte count.
pub(in crate::interpreter) fn eval_vprintf_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [format, array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    super::vsprintf::with_sprintf_array_args(*array, context, values, |args, context, values| {
        let output = super::sprintf::eval_sprintf_bytes(&format, args, context, values)?;
        let len = i64::try_from(output.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
        let output = values.string_bytes_value(&output)?;
        with_eval_value_lease(output, context, values, |output, _, values| values.echo(output))?;
        values.int(len)
    })
}
