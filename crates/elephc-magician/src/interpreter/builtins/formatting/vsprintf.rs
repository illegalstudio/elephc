//! Purpose:
//! Eval registry entry and implementation for `vsprintf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - `vsprintf()` owns array-to-argument expansion for printf-family vector
//!   calls. `vprintf()` reuses that expansion from this file.

use super::super::super::*;

eval_builtin! {
    name: "vsprintf",
    area: Formatting,
    params: [format, values],
    direct: Vsprintf,
    values: Vsprintf,
}

/// Evaluates direct positional `vsprintf()` calls in source order.
pub(in crate::interpreter) fn eval_builtin_vsprintf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_vsprintf_result(&evaluated_args, values)
}

/// Formats `vsprintf()` array arguments and returns the resulting PHP string.
pub(in crate::interpreter) fn eval_vsprintf_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [format, array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    let format_args = eval_sprintf_argument_array_values(*array, values)?;
    let output = super::sprintf::eval_sprintf_bytes(&format, &format_args, values)?;
    values.string_bytes_value(&output)
}

/// Reads `vsprintf()` values in PHP array iteration order while ignoring keys.
pub(in crate::interpreter) fn eval_sprintf_argument_array_values(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if !values.is_array_like(array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(array)?;
    let mut args = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        args.push(values.array_get(array, key)?);
    }
    Ok(args)
}
