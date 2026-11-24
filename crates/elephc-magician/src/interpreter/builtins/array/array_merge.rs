//! Purpose:
//! Declarative eval registry entry for `array_merge`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

eval_builtin! {
    contract: "array_merge",
    area: Array,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_merge` array builtin.
pub(in crate::interpreter) fn eval_array_merge_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_merge(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_merge` array builtin.
pub(in crate::interpreter) fn eval_array_merge_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_merge_result(*left, *right, values)
}

/// Evaluates PHP `array_merge()` over two array expressions.
pub(in crate::interpreter) fn eval_builtin_array_merge(
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
    eval_array_merge_result(left, right, values)
}

/// Builds an `array_merge()` result with PHP numeric reindexing and string-key overwrites.
pub(in crate::interpreter) fn eval_array_merge_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left_len = values.array_len(left)?;
    let right_len = values.array_len(right)?;
    let capacity = left_len
        .checked_add(right_len)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut result = EvalArrayBuilder::assoc(values, capacity)?;
    let mut next_numeric_key = 0_i64;
    for (source, len) in [(left, left_len), (right, right_len)] {
        for position in 0..len {
            result.update(|values, owners, _| {
                let key = owners.stage(values.array_iter_key(source, position)?);
                let value = owners.stage(values.array_get(source, key)?);
                let target_key = if values.type_tag(key)? == EVAL_TAG_STRING {
                    key
                } else {
                    let key = owners.stage(values.int(next_numeric_key)?);
                    next_numeric_key = next_numeric_key.checked_add(1)
                        .ok_or(EvalStatus::RuntimeFatal)?;
                    key
                };
                Ok(Some((target_key, value)))
            })?;
        }
    }
    Ok(result.finish())
}
