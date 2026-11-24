//! Purpose:
//! Declarative eval registry entry and implementation for `array_count_values`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Only `int` and `string` elements are countable; PHP warns and skips anything
//!   else, and eval skips silently because it emits no diagnostics.
//! - Counting through `array_set` means the array layer applies PHP's key
//!   normalization, so the integer `1` and the string `"1"` share one bucket
//!   exactly as they do in the compiled runtime.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

eval_builtin! {
    contract: "array_count_values",
    area: Array,
    direct: Array,
    values: Array,
}

/// Dispatches direct eval calls for the `array_count_values` array builtin.
pub(in crate::interpreter) fn eval_array_count_values_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_count_values(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_count_values` array builtin.
pub(in crate::interpreter) fn eval_array_count_values_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_array_count_values_result(*array, values)
}

/// Evaluates PHP `array_count_values()` over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_count_values(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_count_values_result(array, values)
}

/// Builds the value-to-occurrence-count map PHP's `array_count_values()` returns.
pub(in crate::interpreter) fn eval_array_count_values_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = EvalArrayBuilder::assoc(values, len)?;
    for position in 0..len {
        result.update(|values, owners, result| {
            let key = owners.stage(values.array_iter_key(array, position)?);
            let value = owners.stage(values.array_get(array, key)?);
            if !matches!(values.type_tag(value)?, EVAL_TAG_INT | EVAL_TAG_STRING) {
                return Ok(None);
            }
            let one = owners.stage(values.int(1)?);
            let present = owners.stage(values.array_key_exists(value, result)?);
            let next = if values.truthy(present)? {
                let existing = owners.stage(values.array_get(result, value)?);
                owners.stage(values.add(existing, one)?)
            } else {
                one
            };
            Ok(Some((value, next)))
        })?;
    }
    Ok(result.finish())
}
