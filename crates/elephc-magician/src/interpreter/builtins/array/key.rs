//! Purpose:
//! Declarative eval registry entry and implementation for `key`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - `key()` reads the internal array pointer without moving it, so it takes the
//!   array by value like PHP.
//! - An invalidated pointer answers PHP null.

use super::super::super::*;

eval_builtin! {
    contract: "key",
    area: Array,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `key` array builtin.
pub(in crate::interpreter) fn eval_key_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    super::array_arg_check::eval_check_array_args("key", &[array], context, values)?;
    eval_key_result(array, context, values)
}

/// Dispatches evaluated-argument eval calls for the `key` array builtin.
pub(in crate::interpreter) fn eval_key_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_key_result(*array, context, values)
}

/// Returns the key at the array's internal pointer, or PHP null when invalidated.
pub(in crate::interpreter) fn eval_key_result(
    array: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    match super::array_pointer::eval_array_pointer_position(array, context, values)? {
        Some(position) => values.array_iter_key(array, position),
        None => values.null(),
    }
}
