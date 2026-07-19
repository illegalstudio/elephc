//! Purpose:
//! Eval registry entry and implementation for `ptr_read_string`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Copies raw memory bytes into a PHP byte string.

use std::slice;

use super::super::super::*;


eval_builtin! {
    name: "ptr_read_string",
    area: RawMemory,
    params: [pointer, length],
    direct: PtrReadString,
    values: PtrReadString,
}

/// Evaluates PHP `ptr_read_string()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_ptr_read_string(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer, length] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pointer = eval_expr(pointer, context, scope, values)?;
    let length = eval_expr(length, context, scope, values)?;
    eval_ptr_read_string_result(pointer, length, values)
}

/// Dispatches by-value `ptr_read_string()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_read_string_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer, length] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_ptr_read_string_result(*pointer, *length, values)
}

/// Copies raw memory bytes into a PHP byte string.
fn eval_ptr_read_string_result(
    pointer: RuntimeCellHandle,
    length: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = super::ptr::eval_non_null_pointer(pointer, values)?;
    let length = eval_int_value(length, values)?;
    if length < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
    let bytes = unsafe { slice::from_raw_parts(address.cast::<u8>(), length) };
    values.string_bytes_value(bytes)
}
