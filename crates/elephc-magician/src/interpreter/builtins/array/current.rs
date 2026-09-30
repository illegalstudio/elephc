//! Purpose:
//! Declarative eval registry entry and implementation for `current`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - `current()` reads the internal array pointer without moving it, so it takes
//!   the array by value like PHP.
//! - An invalidated pointer answers PHP false.

use super::super::super::*;

eval_builtin! {
    contract: "current",
    area: Array,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `current` array builtin.
pub(in crate::interpreter) fn eval_current_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    super::array_arg_check::eval_check_array_args("current", &[array], context, values)?;
    eval_current_result(array, context, values)
}

/// Dispatches evaluated-argument eval calls for the `current` array builtin.
pub(in crate::interpreter) fn eval_current_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_current_result(*array, context, values)
}

/// Returns the value at the array's internal pointer, or PHP false when invalidated.
pub(in crate::interpreter) fn eval_current_result(
    array: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(position) = super::array_pointer::eval_array_pointer_position(array, context, values)?
    else {
        return values.bool_value(false);
    };
    let key = values.array_iter_key(array, position)?;
    values.array_get(array, key)
}
