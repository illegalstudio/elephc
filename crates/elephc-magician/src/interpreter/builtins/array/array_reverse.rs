//! Purpose:
//! Declarative eval registry entry for `array_reverse`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-reverse hook.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::{EvalArrayBuilder, with_collection_cells};

eval_builtin! {
    contract: "array_reverse",
    area: Array,
    direct: ArrayReverse,
    values: ArrayReverse,
}
/// Dispatches direct eval calls for the `array_reverse` array builtin.
pub(in crate::interpreter) fn eval_array_reverse_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_reverse(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_reverse` array builtin.
pub(in crate::interpreter) fn eval_array_reverse_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array] => eval_array_reverse_result(*array, false, values),
        [array, preserve_keys] => {
            let preserve_keys = values.truthy(*preserve_keys)?;
            eval_array_reverse_result(*array, preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `array_reverse()` over an eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_reverse(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array] => {
            let array = eval_expr(array, context, scope, values)?;
            eval_array_reverse_result(array, false, values)
        }
        [array, preserve_keys] => {
            let array = eval_expr(array, context, scope, values)?;
            let preserve_keys = eval_expr(preserve_keys, context, scope, values)?;
            let preserve_keys = values.truthy(preserve_keys)?;
            eval_array_reverse_result(array, preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds an `array_reverse()` result while preserving PHP key rules.
pub(in crate::interpreter) fn eval_array_reverse_result(
    array: RuntimeCellHandle,
    preserve_keys: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let has_string_key = if preserve_keys {
        false
    } else {
        with_collection_cells(values, |values, owners| {
            for position in 0..len {
                let key = owners.stage(values.array_iter_key(array, position)?);
                if values.type_tag(key)? == EVAL_TAG_STRING {
                    return Ok(true);
                }
            }
            Ok(false)
        })?
    };
    let mut result = if preserve_keys || has_string_key {
        EvalArrayBuilder::assoc(values, len)?
    } else {
        EvalArrayBuilder::indexed(values, len)?
    };
    let mut next_numeric_key = 0_i64;
    for position in (0..len).rev() {
        result.update(|values, owners, _| {
            let key = owners.stage(values.array_iter_key(array, position)?);
            let value = owners.stage(values.array_get(array, key)?);
            let target_key = if preserve_keys || values.type_tag(key)? == EVAL_TAG_STRING {
                key
            } else {
                let key = owners.stage(values.int(next_numeric_key)?);
                next_numeric_key += 1;
                key
            };
            Ok(Some((target_key, value)))
        })?;
    }
    Ok(result.finish())
}
