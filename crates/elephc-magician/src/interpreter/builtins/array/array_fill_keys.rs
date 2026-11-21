//! Purpose:
//! Declarative eval registry entry for `array_fill_keys`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    contract: "array_fill_keys",
    area: Array,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_fill_keys` array builtin.
pub(in crate::interpreter) fn eval_array_fill_keys_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_fill_keys(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_fill_keys` array builtin.
pub(in crate::interpreter) fn eval_array_fill_keys_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, value] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_fill_keys_result(*keys, *value, context, values)
}

/// Evaluates PHP `array_fill_keys()` over key-array and value expressions.
pub(in crate::interpreter) fn eval_builtin_array_fill_keys(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let keys = eval_expr(keys, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_array_fill_keys_result(keys, value, context, values)
}

/// Builds an `array_fill_keys()` result preserving the source key iteration order.
pub(in crate::interpreter) fn eval_array_fill_keys_result(
    keys: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(keys)?;
    let mut result = super::super::collection_builder::EvalArrayBuilder::assoc(values, len)?;
    for position in 0..len {
        result.entry_key_first(|values| super::key_inputs::key_at(keys, position, false, context, values),
            |values, _| values.retain(value))?;
    }
    Ok(result.finish())
}
