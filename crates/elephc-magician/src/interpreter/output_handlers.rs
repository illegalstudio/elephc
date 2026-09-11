//! Purpose:
//! Bridges runtime-triggered `ob_start()` output-handler invocations back into
//! the interpreter's callable machinery.
//!
//! Called from:
//! - `crate::ffi::ob_handlers::__elephc_eval_ob_handler_v1` (the hook installed
//!   into the generated runtime), never from ordinary builtin dispatch.
//!
//! Key details:
//! - The handler cell was retained at registration time; the returned result
//!   cell is owned by the caller (the runtime unboxes it, maps `false` to
//!   pass-through, and releases it).
//! - Invocation temporaries and detached registry roots each have an explicit cleanup boundary.

use super::builtins::eval_call_user_func_with_values;
use super::eval_release_value;
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
    let mut owners = vec![buffer_cell];
    let result = (|| {
        let phase_cell = values.int(phase)?;
        owners.push(phase_cell);
        eval_call_user_func_with_values(vec![callback, buffer_cell, phase_cell], context, values)
    })();
    let mut cleanup = Ok(());
    for owner in owners {
        if let Err(status) = eval_release_value(context, values, owner) { cleanup = Err(status); }
    }
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value), (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = eval_release_value(context, values, value);
            Err(status)
        }
    }
}

/// Retires detached registry roots while their eval context still supports protected destructors.
#[cfg(not(test))]
pub(crate) fn release_ob_handler_callbacks(
    callbacks: Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut result = Ok(());
    for callback in callbacks {
        if let Err(status) = eval_release_value(context, values, callback) { result = Err(status); }
    }
    result
}
