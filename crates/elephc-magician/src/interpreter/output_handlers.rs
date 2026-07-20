//! Purpose:
//! Bridges runtime-triggered `ob_start()` output-handler invocations back into
//! the interpreter's callable machinery.
//!
//! Called from:
//! - `crate::ffi::ob_handlers::__elephc_eval_ob_handler` (the hook installed
//!   into the generated runtime), never from ordinary builtin dispatch.
//!
//! Key details:
//! - The handler cell was retained at registration time; the returned result
//!   cell is owned by the caller (the runtime unboxes it, maps `false` to
//!   pass-through, and releases it).

use super::builtins::eval_call_user_func_with_values;
use super::RuntimeValueOps;
use crate::abi::ElephcEvalContext;
use crate::errors::EvalStatus;
use crate::value::RuntimeCellHandle;

/// Invokes one eval-registered output handler with `(string $buffer, int $phase)`.
pub(crate) fn eval_ob_handler_callback(
    callback: RuntimeCellHandle,
    buffer: &[u8],
    phase: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let buffer_cell = values.string_bytes_value(buffer)?;
    let phase_cell = values.int(phase)?;
    eval_call_user_func_with_values(vec![callback, buffer_cell, phase_cell], context, values)
}
