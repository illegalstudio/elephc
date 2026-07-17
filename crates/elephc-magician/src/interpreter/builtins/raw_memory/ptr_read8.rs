//! Purpose:
//! Eval registry entry and implementation for `ptr_read8`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Reuses `ptr_get` raw read-width handling for one-byte reads.

use super::super::super::*;


eval_builtin! {
    name: "ptr_read8",
    area: RawMemory,
    params: [pointer],
    direct: PtrRead8,
    values: PtrRead8,
}

/// Evaluates PHP `ptr_read8()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_ptr_read8(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pointer = eval_expr(pointer, context, scope, values)?;
    eval_ptr_read8_result(pointer, values)
}

/// Dispatches by-value `ptr_read8()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_read8_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pointer] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_ptr_read8_result(*pointer, values)
}

/// Reads one raw-memory value for `ptr_read8()`.
fn eval_ptr_read8_result(
    pointer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ptr_get::eval_pointer_read_result(
        pointer,
        super::ptr_get::PointerReadWidth::Byte,
        values,
    )
}
