//! Purpose:
//! Eval registry entry and implementation for `ptr_get`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Owns shared pointer read-width handling for read variants.

use std::ptr;

use super::super::super::*;


eval_builtin! {
    name: "ptr_get",
    area: RawMemory,
    params: [pointer],
    direct: PtrGet,
    values: PtrGet,
}

/// Evaluates PHP `ptr_get()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_ptr_get(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pointer = eval_expr(pointer, context, scope, values)?;
    eval_ptr_get_result(pointer, values)
}

/// Dispatches by-value `ptr_get()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_get_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_ptr_get_result(*pointer, values)
}

/// Reads one raw-memory value for `ptr_get()`.
fn eval_ptr_get_result(
    pointer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_pointer_read_result(pointer, PointerReadWidth::Word64, values)
}

/// Reads one unsigned or machine-word value from raw memory.
pub(super) fn eval_pointer_read_result(
    pointer: RuntimeCellHandle,
    width: PointerReadWidth,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = super::ptr::eval_non_null_pointer(pointer, values)?;
    let value = unsafe {
        match width {
            PointerReadWidth::Byte => i64::from(ptr::read_unaligned(address.cast::<u8>())),
            PointerReadWidth::Half => i64::from(ptr::read_unaligned(address.cast::<u16>())),
            PointerReadWidth::Word32 => i64::from(ptr::read_unaligned(address.cast::<u32>())),
            PointerReadWidth::Word64 => {
                let word = ptr::read_unaligned(address.cast::<u64>());
                i64::from_ne_bytes(word.to_ne_bytes())
            }
        }
    };
    values.int(value)
}

/// Widths supported by pointer read helpers.
pub(super) enum PointerReadWidth {
    Byte,
    Half,
    Word32,
    Word64,
}
