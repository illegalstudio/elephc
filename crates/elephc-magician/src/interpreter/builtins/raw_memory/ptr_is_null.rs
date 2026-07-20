//! Purpose:
//! Eval registry entry and implementation for `ptr_is_null`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Checks the integer raw-address representation against zero.

use super::super::super::*;


eval_builtin! {
    name: "ptr_is_null",
    area: RawMemory,
    params: [pointer],
    direct: PtrIsNull,
    values: PtrIsNull,
}

/// Evaluates PHP `ptr_is_null()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_ptr_is_null(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pointer = eval_expr(pointer, context, scope, values)?;
    eval_ptr_is_null_result(pointer, values)
}

/// Dispatches by-value `ptr_is_null()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_is_null_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_ptr_is_null_result(*pointer, values)
}

/// Returns whether one raw pointer address is null.
fn eval_ptr_is_null_result(
    pointer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = super::ptr::eval_pointer_address(pointer, values)?;
    values.bool_value(address == 0)
}
