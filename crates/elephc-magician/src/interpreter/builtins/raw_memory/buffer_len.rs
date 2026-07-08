//! Purpose:
//! Eval registry entry and implementation for `buffer_len`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Reads the logical element count from an AOT-shaped buffer header.

use std::ptr;

use super::super::super::*;


eval_builtin! {
    name: "buffer_len",
    area: RawMemory,
    params: [buffer],
    direct: BufferLen,
    values: BufferLen,
}

/// Evaluates PHP `buffer_len()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_buffer_len(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [buffer] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let buffer = eval_expr(buffer, context, scope, values)?;
    eval_buffer_len_result(buffer, values)
}

/// Dispatches by-value `buffer_len()` calls after argument binding.
pub(in crate::interpreter) fn eval_buffer_len_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [buffer] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_buffer_len_result(*buffer, values)
}

/// Reads the logical element count from an AOT-shaped buffer header.
fn eval_buffer_len_result(
    buffer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let header = super::ptr::eval_non_null_pointer(buffer, values)?.cast::<usize>();
    let length = unsafe { ptr::read(header) };
    values.int(i64::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?)
}
