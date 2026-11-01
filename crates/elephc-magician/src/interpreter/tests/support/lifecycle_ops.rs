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

    /// Gives one reference back, recording the release and failing an over-release when counted.
    ///
    /// The handle is not freed, because tests read released cells to assert what they held.
    pub(super) fn runtime_release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        self.releases.push(value);
        let count_after = self.adjust_refcount(value, -1);
        if count_after < 0 {
            let handle = value.as_ptr() as usize;
            let held = self
                .values
                .get(&handle)
                .map_or_else(|| "<unallocated>".to_string(), |value| format!("{value:?}"));
            self.over_releases.push(FakeOverRelease {
                handle,
                count_after,
                value: held.clone(),
            });
            assert!(
                !self.counting_enforced(),
                "released a fake cell nobody owned: handle {handle} reached count {count_after} \
                 holding {held}. Something gave back a reference it never took -- find the \
                 releasing site in the backtrace below and either stop releasing there or retain \
                 first.\n{}",
                std::backtrace::Backtrace::force_capture()
            );
        }
        if let Some(owners) = self.cell_owners.get_mut(&(value.as_ptr() as usize)) {
            *owners = owners.saturating_sub(1);
        }
        if self.fail_release_call == Some(self.releases.len() - 1) {
            return Err(EvalStatus::UncaughtThrowable);
        }
        Ok(())
    }
    /// Takes one more reference to a fake cell and returns the same handle.
    /// Records the retained lease in `retains` and `cell_owners`.
    pub(super) fn runtime_retain(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.adjust_refcount(value, 1);
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
