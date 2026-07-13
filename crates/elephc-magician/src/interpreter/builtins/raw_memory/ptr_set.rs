//! Purpose:
//! Eval registry entry and implementation for `ptr_set`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Owns shared pointer write-width handling for write variants.

use std::ptr;

use super::super::super::*;


eval_builtin! {
    name: "ptr_set",
    area: RawMemory,
    params: [pointer, value],
    direct: PtrSet,
    values: PtrSet,
}

/// Evaluates PHP `ptr_set()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_ptr_set(
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
    eval_ptr_set_result(pointer, value, values)
}

/// Dispatches by-value `ptr_set()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_set_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer, value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_ptr_set_result(*pointer, *value, values)
}

/// Writes one raw-memory value for `ptr_set()`.
fn eval_ptr_set_result(
    pointer: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_pointer_write_result(pointer, value, PointerWriteWidth::Word64, values)
}

/// Writes one integer payload to raw memory and returns PHP null.
pub(super) fn eval_pointer_write_result(
    pointer: RuntimeCellHandle,
    value: RuntimeCellHandle,
    width: PointerWriteWidth,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = super::ptr::eval_non_null_pointer(pointer, values)?;
    let value = eval_int_value(value, values)?;
    unsafe {
        match width {
            PointerWriteWidth::Byte => ptr::write_unaligned(address.cast::<u8>(), value as u8),
            PointerWriteWidth::Half => ptr::write_unaligned(address.cast::<u16>(), value as u16),
            PointerWriteWidth::Word32 => ptr::write_unaligned(address.cast::<u32>(), value as u32),
            PointerWriteWidth::Word64 => {
                ptr::write_unaligned(
                    address.cast::<u64>(),
                    u64::from_ne_bytes(value.to_ne_bytes()),
                )
            }
        }
    }
    values.null()
}

/// Widths supported by pointer write helpers.
pub(super) enum PointerWriteWidth {
    Byte,
    Half,
    Word32,
    Word64,
}
