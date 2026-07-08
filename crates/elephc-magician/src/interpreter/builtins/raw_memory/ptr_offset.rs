//! Purpose:
//! Eval registry entry and implementation for `ptr_offset`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Performs checked signed byte-offset arithmetic on raw pointer addresses.

use super::super::super::*;


eval_builtin! {
    name: "ptr_offset",
    area: RawMemory,
    params: [pointer, offset],
    direct: PtrOffset,
    values: PtrOffset,
}

/// Evaluates PHP `ptr_offset()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_ptr_offset(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer, offset] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pointer = eval_expr(pointer, context, scope, values)?;
    let offset = eval_expr(offset, context, scope, values)?;
    eval_ptr_offset_result(pointer, offset, values)
}

/// Dispatches by-value `ptr_offset()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_offset_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer, offset] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_ptr_offset_result(*pointer, *offset, values)
}

/// Computes a derived raw pointer address by adding a signed byte offset.
fn eval_ptr_offset_result(
    pointer: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = super::ptr::eval_pointer_address(pointer, values)?;
    let offset = eval_int_value(offset, values)?;
    let address = if offset >= 0 {
        let offset = usize::try_from(offset).map_err(|_| EvalStatus::RuntimeFatal)?;
        address.checked_add(offset).ok_or(EvalStatus::RuntimeFatal)?
    } else {
        let offset = usize::try_from(offset.unsigned_abs()).map_err(|_| EvalStatus::RuntimeFatal)?;
        address.checked_sub(offset).ok_or(EvalStatus::RuntimeFatal)?
    };
    super::ptr::eval_address_value(address, values)
}
