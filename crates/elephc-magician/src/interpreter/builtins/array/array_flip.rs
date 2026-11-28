//! Purpose:
//! Declarative eval registry entry for `array_flip`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-flip hook.

use super::super::super::*;

eval_builtin! {
    contract: "array_flip",
    area: Array,
    direct: ArrayFlip,
    values: ArrayFlip,
}
/// Dispatches direct eval calls for the `array_flip` array builtin.
pub(in crate::interpreter) fn eval_array_flip_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_flip(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_flip` array builtin.
pub(in crate::interpreter) fn eval_array_flip_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_flip_result(*array, values)
}

/// Evaluates PHP `array_flip()` over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_flip(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_flip_result(array, values)
}

/// Builds the associative result for `array_flip()` using PHP's valid value-key subset.
pub(in crate::interpreter) fn eval_array_flip_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = super::super::collection_builder::EvalArrayBuilder::assoc(values, len)?;
    for position in 0..len {
        let Some(key) = super::key_inputs::countable_key_at(array, position, result.values())? else { continue; };
        result.entry_key_first(|_| Ok(key), |values, _| values.array_iter_key(array, position))?;
    }
    Ok(result.finish())
}
