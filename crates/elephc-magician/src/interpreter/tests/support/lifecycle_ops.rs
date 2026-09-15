//! Purpose:
//! Release, retain, warning, and echo fake runtime operations.
//!
//! Called from:
//! - `crate::interpreter::tests::support::runtime_ops`.
//!
//! Key details:
//! - These helpers record observable side effects for assertions without touching real runtime memory.

use super::*;

impl FakeOps {
    /// Allocates reference markers with optional failure after earlier slots were staged.
    pub(super) fn runtime_invoker_marker(&mut self, slot: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        let call = self.invoker_marker_calls;
        self.invoker_marker_calls += 1;
        if self.fail_invoker_marker_call == Some(call) { return Err(EvalStatus::RuntimeFatal); }
        Ok(self.alloc(FakeValue::InvokerRefCell(slot)))
    }

    /// Records fake releases without freeing handles needed for assertions.
    pub(super) fn runtime_release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        self.releases.push(value);
        if let Some(owners) = self.cell_owners.get_mut(&(value.as_ptr() as usize)) {
            *owners = owners.saturating_sub(1);
        }
        if self.fail_release_call == Some(self.releases.len() - 1) {
            return Err(EvalStatus::UncaughtThrowable);
        }
        Ok(())
    }
    /// Records a retained lease while preserving fake cell identity.
    pub(super) fn runtime_retain(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.retains.push(value);
        *self.cell_owners.entry(value.as_ptr() as usize).or_default() += 1;
        Ok(value.owned())
    }
    /// Records fake PHP warnings without writing to stderr.
    pub(super) fn runtime_warning(&mut self, message: &str) -> Result<(), EvalStatus> {
        self.warnings.push(message.to_string());
        Ok(())
    }
    /// Appends fake echo output for interpreter tests, honoring the fake ob_* stack.
    pub(super) fn runtime_echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        let value = self.stringify(value);
        match self.ob_stack.last_mut() {
            Some(level) => level.buffer.push_str(&value),
            None => self.output.push_str(&value),
        }
        Ok(())
    }
}
