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
    /// Cleans native arguments and discards an interrupted result without replacing a pending throw.
    pub(super) fn finish_native_call(
        &mut self,
        result: *mut RuntimeCell,
        arguments: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let outcome = self.handle_native_call_result(result);
        if let Err(status) = self.release_cells([arguments]) {
            if let Ok(value) = outcome {
                self.release_cells([value])?;
            }
            return Err(status);
        }
        outcome
    }

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

    /// Takes the owned Throwable box transferred by a generated native call boundary.
    pub(super) fn take_pending_native_throwable(&self) -> Option<RuntimeCellHandle> {
        let thrown = unsafe { __elephc_eval_value_take_pending_throwable() };
        if thrown.is_null() {
            None
        } else {
            Some(RuntimeCellHandle::from_raw(thrown))
        }
    }

    /// Schedules a native Throwable so eval's ordinary catch machinery can handle it.
    pub(in crate::runtime_hooks) fn schedule_pending_throw(
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
