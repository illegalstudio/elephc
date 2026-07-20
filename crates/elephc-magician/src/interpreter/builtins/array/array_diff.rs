//! Purpose:
//! Declarative eval registry entry for `array_diff`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    name: "array_diff",
    area: Array,
    params: [array],
    variadic: arrays,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_diff` array builtin.
pub(in crate::interpreter) fn eval_array_diff_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_value_set("array_diff", args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_diff` array builtin.
pub(in crate::interpreter) fn eval_array_diff_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_value_set_result("array_diff", *left, *right, values)
}

/// Evaluates PHP value-set array builtins over two eval array expressions.
pub(in crate::interpreter) fn eval_builtin_array_value_set(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_array_value_set_result(name, left, right, values)
}

/// Builds `array_diff()` or `array_intersect()` using PHP's default string comparison mode.
pub(in crate::interpreter) fn eval_array_value_set_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left_len = values.array_len(left)?;
    let right_len = values.array_len(right)?;
    let mut right_values = Vec::with_capacity(right_len);
    for position in 0..right_len {
        let key = values.array_iter_key(right, position)?;
        let value = values.array_get(right, key)?;
        right_values.push(values.string_bytes(value)?);
    }

    let mut result = values.assoc_new(left_len)?;
    for position in 0..left_len {
        let key = values.array_iter_key(left, position)?;
        let value = values.array_get(left, key)?;
        let comparable = values.string_bytes(value)?;
        let found = right_values
            .iter()
            .any(|right_value| right_value == &comparable);
        let keep = match name {
            "array_diff" => !found,
            "array_intersect" => found,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        if keep {
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}
