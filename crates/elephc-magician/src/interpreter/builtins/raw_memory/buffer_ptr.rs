//! Purpose:
//! Eval registry entry and implementation for `buffer_ptr`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - An eval buffer value is an opaque registry handle, so this builtin is the only way to
//!   its payload; a freed buffer or a stale alias dies with the use-after-free fatal.

use super::super::super::*;


eval_builtin! {
    contract: "buffer_ptr",
    area: RawMemory,
    direct: BufferPtr,
    values: BufferPtr,
}

/// Evaluates PHP `buffer_ptr()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_buffer_ptr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [buffer] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let buffer = eval_expr(buffer, context, scope, values)?;
    eval_buffer_ptr_result(buffer, values)
}

/// Dispatches by-value `buffer_ptr()` calls after argument binding.
pub(in crate::interpreter) fn eval_buffer_ptr_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [buffer] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_buffer_ptr_result(*buffer, values)
}

/// Answers the payload address of a live eval buffer.
fn eval_buffer_ptr_result(
    buffer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let payload = super::buffer_registry::eval_buffer_resolve(buffer, values)?.payload;
    super::ptr::eval_address_value(payload, values)
}
