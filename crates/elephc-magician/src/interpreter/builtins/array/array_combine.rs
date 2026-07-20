//! Purpose:
//! Declarative eval registry entry for `array_combine`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    name: "array_combine",
    area: Array,
    params: [keys, values],
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_combine` array builtin.
pub(in crate::interpreter) fn eval_array_combine_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_combine(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_combine` array builtin.
pub(in crate::interpreter) fn eval_array_combine_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, values_array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_combine_result(*keys, *values_array, values)
}

/// Evaluates PHP `array_combine()` over key and value array expressions.
pub(in crate::interpreter) fn eval_builtin_array_combine(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, values_array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let keys = eval_expr(keys, context, scope, values)?;
    let values_array = eval_expr(values_array, context, scope, values)?;
    eval_array_combine_result(keys, values_array, values)
}

/// Builds the associative result for `array_combine()` from two eval arrays.
pub(in crate::interpreter) fn eval_array_combine_result(
    keys: RuntimeCellHandle,
    values_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(keys)?;
    if len != values.array_len(values_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }

    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let source_key = values.array_iter_key(keys, position)?;
        let target_key = values.array_get(keys, source_key)?;
        let target_key = values.cast_string(target_key)?;
        let value_key = values.array_iter_key(values_array, position)?;
        let value = values.array_get(values_array, value_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}
