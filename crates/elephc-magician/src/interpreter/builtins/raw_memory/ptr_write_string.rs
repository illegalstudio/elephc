//! Purpose:
//! Eval registry entry and implementation for `ptr_write_string`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Copies PHP string bytes into raw memory and returns the byte count written.

use std::ptr;

use super::super::super::*;


eval_builtin! {
    name: "ptr_write_string",
    area: RawMemory,
    params: [pointer, string],
    direct: PtrWriteString,
    values: PtrWriteString,
}

/// Evaluates PHP `ptr_write_string()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_ptr_write_string(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer, string] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pointer = eval_expr(pointer, context, scope, values)?;
    let string = eval_expr(string, context, scope, values)?;
    eval_ptr_write_string_result(pointer, string, values)
}

/// Dispatches by-value `ptr_write_string()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_write_string_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer, string] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_ptr_write_string_result(*pointer, *string, values)
}

/// Copies PHP string bytes into raw memory and returns the byte count written.
fn eval_ptr_write_string_result(
    pointer: RuntimeCellHandle,
    string: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = super::ptr::eval_non_null_pointer(pointer, values)?;
    let bytes = values.string_bytes(string)?;
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), address.cast::<u8>(), bytes.len());
    }
    values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?)
}
