//! Purpose:
//! Eval registry entry and implementation for `array_key_first`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array` dispatch tables.
//!
//! Key details:
//! - `null` for an empty array, else the first key in iteration order.

use super::super::super::*;

eval_builtin! {
    contract: "array_key_first",
    area: Array,
    direct: Array,
    values: Array,
}

/// Dispatches direct eval calls for `array_key_first()`.
pub(in crate::interpreter) fn eval_array_key_first_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_key_first_result(array, values)
}

/// Dispatches evaluated-argument eval calls for `array_key_first()`.
pub(in crate::interpreter) fn eval_array_key_first_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_array_key_first_result(*array, values)
}

/// Answers `array_key_first()`.
fn eval_array_key_first_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.array_len(array)? == 0 {
        return values.null();
    }
    values.array_iter_key(array, 0)
}
