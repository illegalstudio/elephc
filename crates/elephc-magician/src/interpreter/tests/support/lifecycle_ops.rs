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
    /// Records fake releases without freeing handles needed for assertions.
    pub(super) fn runtime_release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        self.releases.push(value);
        Ok(())
    }
    /// Returns the same fake handle because fake cells do not refcount.
    pub(super) fn runtime_retain(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(value)
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
