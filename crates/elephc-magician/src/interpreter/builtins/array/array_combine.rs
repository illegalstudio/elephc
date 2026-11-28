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
    contract: "array_combine",
    area: Array,
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
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, values_array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_combine_result(*keys, *values_array, context, values)
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
    eval_array_combine_result(keys, values_array, context, values)
}

/// Builds the associative result for `array_combine()` from two eval arrays.
pub(in crate::interpreter) fn eval_array_combine_result(
    keys: RuntimeCellHandle,
    values_array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(keys)?;
    if len != values.array_len(values_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }

    let mut result = super::super::collection_builder::EvalArrayBuilder::assoc(values, len)?;
    for position in 0..len {
        result.entry_key_first(|values| super::key_inputs::key_at(keys, position, true, context, values),
            |values, _| super::key_inputs::value_at(values_array, position, values))?;
    }
    Ok(result.finish())
}
