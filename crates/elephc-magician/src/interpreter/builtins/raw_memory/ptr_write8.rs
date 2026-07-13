//! Purpose:
//! Eval registry entry and implementation for `ptr_write8`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Reuses `ptr_set` raw write-width handling for one-byte writes.

use super::super::super::*;


eval_builtin! {
    name: "ptr_write8",
    area: RawMemory,
    params: [pointer, value],
    direct: PtrWrite8,
    values: PtrWrite8,
}

/// Evaluates PHP `ptr_write8()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_ptr_write8(
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
    eval_ptr_write8_result(pointer, value, values)
}

/// Dispatches by-value `ptr_write8()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_write8_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer, value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_ptr_write8_result(*pointer, *value, values)
}

/// Writes one raw-memory value for `ptr_write8()`.
fn eval_ptr_write8_result(
    pointer: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ptr_set::eval_pointer_write_result(
        pointer,
        value,
        super::ptr_set::PointerWriteWidth::Byte,
        values,
    )
}
