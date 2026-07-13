//! Purpose:
//! Eval registry entry and implementation for `buffer_new`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - `buffer_new()` returns the same header pointer shape used by AOT buffers:
//!   length word, stride word, then zeroed payload.

use std::mem;
use std::ptr;

use super::super::super::*;


eval_builtin! {
    name: "buffer_new",
    area: RawMemory,
    params: [length],
    direct: BufferNew,
    values: BufferNew,
}

const BUFFER_HEADER_WORDS: usize = 2;
const BUFFER_DEFAULT_STRIDE: usize = 8;

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

/// Allocates a zero-filled AOT-shaped buffer and returns its header address.
fn eval_buffer_new_result(
    length: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let length = eval_int_value(length, values)?;
    if length < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
    let header_bytes = BUFFER_HEADER_WORDS
        .checked_mul(mem::size_of::<usize>())
        .ok_or(EvalStatus::RuntimeFatal)?;
    let payload_bytes = length
        .checked_mul(BUFFER_DEFAULT_STRIDE)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let total_bytes = header_bytes
        .checked_add(payload_bytes)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let allocation = unsafe { libc::calloc(total_bytes.max(1), 1) };
    if allocation.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    unsafe {
        let header = allocation.cast::<usize>();
        ptr::write(header, length);
        ptr::write(header.add(1), BUFFER_DEFAULT_STRIDE);
    }
    super::ptr::eval_address_value(allocation as usize, values)
}
