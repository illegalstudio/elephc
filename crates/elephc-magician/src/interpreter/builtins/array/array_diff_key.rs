//! Purpose:
//! Declarative eval registry entry for `array_diff_key`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    name: "array_diff_key",
    area: Array,
    params: [array],
    variadic: arrays,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_diff_key` array builtin.
pub(in crate::interpreter) fn eval_array_diff_key_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_key_set("array_diff_key", args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_diff_key` array builtin.
pub(in crate::interpreter) fn eval_array_diff_key_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_key_set_result("array_diff_key", *left, *right, values)
}

/// Evaluates PHP key-set array builtins over two eval array expressions.
pub(in crate::interpreter) fn eval_builtin_array_key_set(
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
    eval_array_key_set_result(name, left, right, values)
}

/// Builds `array_diff_key()` or `array_intersect_key()` by testing first-array keys.
pub(in crate::interpreter) fn eval_array_key_set_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left_len = values.array_len(left)?;
    let mut result = values.assoc_new(left_len)?;
    for position in 0..left_len {
        let key = values.array_iter_key(left, position)?;
        let value = values.array_get(left, key)?;
        let exists = values.array_key_exists(key, right)?;
        let found = values.truthy(exists)?;
        let keep = match name {
            "array_diff_key" => !found,
            "array_intersect_key" => found,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        if keep {
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}
