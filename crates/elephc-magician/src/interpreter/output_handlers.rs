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
use crate::runtime_hooks::owner_frames::{
    RuntimeOwnerSlot, StagedRuntimeOwners, with_runtime_owners,
};

/// Invokes one eval-registered output handler with `(string $buffer, int $phase)`.
pub(crate) fn eval_ob_handler_callback(
    callback: RuntimeCellHandle,
    buffer: &[u8],
    phase: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    use elephc_memoir::runtime_owners::{CleanupErrorPolicy, ReleaseOrder};
    let owners = StagedRuntimeOwners::default();
    let output = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&owners, &output), || {
        let operation = (|| {
            let buffer_cell = owners.stage(values.string_bytes_value(buffer)?);
            let phase_cell = owners.stage(values.int(phase)?);
            let result = eval_call_user_func_with_values(
                vec![callback, buffer_cell.borrowed(), phase_cell.borrowed()],
                context,
                values,
            )?;
            output.replace(Some(result));
            Ok(())
        })();
        let cleanup = owners.take_all().release(
            ReleaseOrder::Forward,
            CleanupErrorPolicy::Last,
            |value| eval_release_value(context, values, value),
        );
        output.finish(operation.and(cleanup), |value| eval_release_value(context, values, value))
    })
}

/// Retires detached registry roots while their eval context still supports protected destructors.
#[cfg(not(test))]
pub(crate) fn release_ob_handler_callbacks(
    callbacks: elephc_memoir::runtime_owners::RuntimeOwnerBatch,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    use elephc_memoir::runtime_owners::{CleanupErrorPolicy, ReleaseOrder};
    with_runtime_owners(&callbacks, || {
        callbacks.release(ReleaseOrder::Forward, CleanupErrorPolicy::Last, |callback| {
            eval_release_value(context, values, callback)
        })
    })
}
