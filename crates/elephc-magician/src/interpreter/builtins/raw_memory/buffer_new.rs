//! Purpose:
//! Eval registry entry and implementation for `buffer_new`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - `buffer_new()` returns an opaque handle from the generation-checked eval buffer
//!   registry (`buffer_registry`), never the address of its zeroed word-element payload.

use super::super::super::*;


eval_builtin! {
    contract: "buffer_new",
    area: RawMemory,
    direct: BufferNew,
    values: BufferNew,
}

/// Evaluates PHP `buffer_new()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_buffer_new(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [length] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let length = eval_expr(length, context, scope, values)?;
    eval_buffer_new_result(length, values)
}

/// Dispatches by-value `buffer_new()` calls after argument binding.
pub(in crate::interpreter) fn eval_buffer_new_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [length] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_buffer_new_result(*length, values)
}

/// Allocates a zero-filled buffer in the eval buffer registry and returns its handle.
fn eval_buffer_new_result(
    length: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let length = eval_int_value(length, values)?;
    if length < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
    let handle = super::buffer_registry::eval_buffer_allocate(length)?;
    values.int(handle)
}
