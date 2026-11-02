//! Purpose:
//! Eval registry entry and implementation for `ptr_read_f64`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Reads one IEEE 754 double through the raw pointer and returns it as a PHP float.

use std::ptr;

use super::super::super::*;


eval_builtin! {
    contract: "ptr_read_f64",
    area: RawMemory,
    direct: PtrReadF64,
    values: PtrReadF64,
}

/// Evaluates PHP `ptr_read_f64()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_ptr_read_f64(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pointer = eval_expr(pointer, context, scope, values)?;
    eval_ptr_read_f64_result(pointer, values)
}

/// Dispatches by-value `ptr_read_f64()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_read_f64_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_ptr_read_f64_result(*pointer, values)
}

/// Reads one IEEE 754 double for `ptr_read_f64()` and widens it to the PHP float.
fn eval_ptr_read_f64_result(
    pointer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = super::ptr::eval_non_null_pointer(pointer, values)?;
    let value = unsafe { ptr::read_unaligned(address.cast::<f64>()) };
    values.float(value)
}
