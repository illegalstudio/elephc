//! Purpose:
//! Converts null native bridge results into pending Throwable state and schedules
//! escaped native exceptions for eval catch handling.
//!
//! Called from:
//! - Runtime method and constructor bridge operations.
//!
//! Key details:
//! - Null without a pending Throwable remains a runtime fatal error.

use super::*;

#[cfg(not(test))]
impl ElephcRuntimeOps {
    /// Converts a generated native method-call result into an eval result status.
    pub(super) fn handle_native_call_result(
        &self,
        result: *mut RuntimeCell,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        if !result.is_null() {
            return Ok(RuntimeCellHandle::from_raw(result));
        }
        self.take_pending_native_throwable()
            .map_or(Err(EvalStatus::RuntimeFatal), |thrown| {
                self.schedule_pending_throw(thrown)?;
                Err(EvalStatus::UncaughtThrowable)
            })
    }

    /// Takes a native Throwable that escaped through the generated constructor bridge.
    pub(super) fn take_pending_native_throwable(&self) -> Option<RuntimeCellHandle> {
        let thrown = unsafe { __elephc_eval_value_take_pending_throwable() };
        if thrown.is_null() {
            None
        } else {
            Self::object_from_raw(thrown).ok()
        }
    }

    /// Schedules a native Throwable so eval's ordinary catch machinery can handle it.
    pub(super) fn schedule_pending_throw(
        &self,
        thrown: RuntimeCellHandle,
    ) -> Result<(), EvalStatus> {
        let Some(context) =
            (unsafe { (self.context as *mut crate::abi::ElephcEvalContext).as_mut() })
        else {
            return Err(EvalStatus::RuntimeFatal);
        };
        context.set_pending_throw(thrown);
        Ok(())
    }
}
