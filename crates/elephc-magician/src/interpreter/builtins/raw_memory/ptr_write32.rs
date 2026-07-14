//! Purpose:
//! Eval registry entry and implementation for `ptr_write32`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Reuses `ptr_set` raw write-width handling for four-byte writes.

use super::super::super::*;


eval_builtin! {
    name: "ptr_write32",
    area: RawMemory,
    params: [pointer, value],
    direct: PtrWrite32,
    values: PtrWrite32,
}

/// Evaluates PHP `ptr_write32()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_ptr_write32(
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
    eval_ptr_write32_result(pointer, value, values)
}

/// Dispatches by-value `ptr_write32()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_write32_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer, value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_ptr_write32_result(*pointer, *value, values)
}

/// Writes one raw-memory value for `ptr_write32()`.
fn eval_ptr_write32_result(
    pointer: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ptr_set::eval_pointer_write_result(
        pointer,
        value,
        super::ptr_set::PointerWriteWidth::Word32,
        values,
    )
}
