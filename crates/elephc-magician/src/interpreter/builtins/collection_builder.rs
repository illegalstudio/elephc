//! Purpose:
//! Builds eval-owned array results without leaking temporary keys or values.
//!
//! Called from:
//! - Core introspection, backtrace, and class-default builtin adapters.
//!
//! Key details:
//! - Runtime array setters borrow their operands and retain stored values.
//! - An unfinished builder releases its array, including already inserted entries.

use crate::errors::EvalStatus;
use crate::interpreter::RuntimeValueOps;
use crate::value::RuntimeCellHandle;

/// Owns one result array until it is returned to the caller or abandoned on error.
pub(in crate::interpreter) struct EvalArrayBuilder<'a, V: RuntimeValueOps> {
    cell: Option<RuntimeCellHandle>,
    values: &'a mut V,
}

impl<'a, V: RuntimeValueOps> EvalArrayBuilder<'a, V> {
    /// Allocates an owned associative result with the requested initial capacity.
    pub(in crate::interpreter) fn assoc(values: &'a mut V, capacity: usize) -> Result<Self, EvalStatus> {
        let cell = values.assoc_new(capacity)?;
        Ok(Self::from_owned(values, cell))
    }

    /// Allocates an owned indexed result with the requested initial capacity.
    pub(in crate::interpreter) fn indexed(values: &'a mut V, capacity: usize) -> Result<Self, EvalStatus> {
        let cell = values.array_new(capacity)?;
        Ok(Self::from_owned(values, cell))
    }

    /// Takes over an existing owned result without retaining it again.
    pub(in crate::interpreter) fn from_owned(values: &'a mut V, cell: RuntimeCellHandle) -> Self {
        Self { cell: Some(cell), values }
    }

    /// Temporarily lends the runtime operations while keeping the result array owned.
    pub(in crate::interpreter) fn values(&mut self) -> &mut V {
        self.values
    }

    /// Inserts a freshly materialized value under a freshly boxed string key.
    pub(in crate::interpreter) fn string(
        &mut self,
        key: &str,
        value: impl FnOnce(&mut V) -> Result<RuntimeCellHandle, EvalStatus>,
    ) -> Result<(), EvalStatus> {
        self.entry(value, |values, _| values.string(key))
    }

    /// Inserts a freshly materialized value at an integer position.
    pub(in crate::interpreter) fn index(
        &mut self,
        position: usize,
        value: impl FnOnce(&mut V) -> Result<RuntimeCellHandle, EvalStatus>,
    ) -> Result<(), EvalStatus> {
        self.entry(value, |values, _| {
            values.int(i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?)
        })
    }

    /// Consumes an owned value and key, releasing both even when key creation or insertion fails.
    /// The key closure borrows the value, allowing resource ids to be derived without reboxing.
    pub(in crate::interpreter) fn entry(
        &mut self,
        value: impl FnOnce(&mut V) -> Result<RuntimeCellHandle, EvalStatus>,
        key: impl FnOnce(&mut V, RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>,
    ) -> Result<(), EvalStatus> {
        let value = value(self.values)?;
        let key = match key(self.values, value) {
            Ok(key) => key,
            Err(status) => {
                let _ = self.values.release(value);
                return Err(status);
            }
        };
        let inserted = self.values.array_set(self.cell.unwrap(), key, value);
        let key_released = self.values.release(key);
        let value_released = self.values.release(value);
        self.cell = Some(inserted?);
        key_released?;
        value_released
    }

    /// Transfers the finished array to the caller without releasing its entries.
    pub(in crate::interpreter) fn finish(mut self) -> RuntimeCellHandle {
        self.cell.take().unwrap()
    }
}

impl<V: RuntimeValueOps> Drop for EvalArrayBuilder<'_, V> {
    /// Releases unfinished arrays on early returns, including nested builder failures.
    fn drop(&mut self) {
        if let Some(cell) = self.cell.take() {
            let _ = self.values.release(cell);
        }
    }
}
