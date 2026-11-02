//! Purpose:
//! Eval registry entry and implementation for `array_is_list`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array` dispatch tables.
//!
//! Key details:
//! - True when the keys are exactly 0, 1, 2, ... in iteration order, as php checks.

use super::super::super::*;

eval_builtin! {
    contract: "array_is_list",
    area: Array,
    direct: Array,
    values: Array,
}

/// Dispatches direct eval calls for `array_is_list()`.
pub(in crate::interpreter) fn eval_array_is_list_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_is_list_result(array, values)
}

/// Dispatches evaluated-argument eval calls for `array_is_list()`.
pub(in crate::interpreter) fn eval_array_is_list_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_array_is_list_result(*array, values)
}

/// Answers `array_is_list()`: every key, in iteration order, equals its position.
fn eval_array_is_list_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let is_position = values.type_tag(key)? == EVAL_TAG_INT
            && values.raw_value_word(key)? as i64 == position as i64;
        if !is_position {
            return values.bool_value(false);
        }
    }
    values.bool_value(true)
}
