//! Purpose:
//! Eval registry entry and implementation for `ptr_write_f64`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Casts the value to a PHP float and reads its exact payload bits (no string round
//!   trip), then stores it as an IEEE 754 double through the raw pointer.

use std::ptr;

use super::super::super::*;


eval_builtin! {
    contract: "ptr_write_f64",
    area: RawMemory,
    direct: PtrWriteF64,
    values: PtrWriteF64,
}

/// Evaluates PHP `ptr_write_f64()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_ptr_write_f64(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pointer = eval_expr(pointer, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_ptr_write_f64_result(pointer, value, values)
}

/// Dispatches by-value `ptr_write_f64()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_write_f64_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer, value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_ptr_write_f64_result(*pointer, *value, values)
}

/// Stores one IEEE 754 double for `ptr_write_f64()`.
fn eval_ptr_write_f64_result(
    pointer: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = super::ptr::eval_non_null_pointer(pointer, values)?;
    let value = super::ptr_set::eval_exact_float_payload(value, values)?;
    unsafe { ptr::write_unaligned(address.cast::<f64>(), value) };
    values.null()
}
