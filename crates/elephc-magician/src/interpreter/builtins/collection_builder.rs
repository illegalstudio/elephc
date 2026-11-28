//! Purpose:
//! Builds eval-owned array results without leaking temporary keys or values.
//!
//! Called from:
//! - Core introspection, backtrace, and class-default builtin adapters.
//!
//! Key details:
//! - Runtime array setters borrow their operands and retain stored values.
//! - An unfinished builder releases its array, including already inserted entries.

use crate::context::ElephcEvalContext;
use crate::errors::EvalStatus;
use crate::interpreter::{RuntimeValueOps, operands::release_expr_result};
use crate::value::RuntimeCellHandle;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, StagedRuntimeOwners, with_runtime_owners};
use elephc_memoir::runtime_owners::release_staged_cells;

/// Owns one result array until it is returned to the caller or abandoned on error.
pub(in crate::interpreter) struct EvalArrayBuilder<'a, V: RuntimeValueOps> {
    cell: RuntimeOwnerSlot,
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

    /// Allocates a direct object array for Reflection's compiled readers.
    pub(in crate::interpreter) fn objects(values: &'a mut V, capacity: usize) -> Result<Self, EvalStatus> {
        let cell = values.object_array_new(capacity)?;
        Ok(Self::from_owned(values, cell))
    }

    /// Takes over an existing owned result without retaining it again.
    pub(in crate::interpreter) fn from_owned(values: &'a mut V, cell: RuntimeCellHandle) -> Self {
        Self { cell: RuntimeOwnerSlot::new(cell), values }
    }

    /// Temporarily lends the runtime operations while keeping the result array owned.
    pub(in crate::interpreter) fn values(&mut self) -> &mut V {
        self.values
    }

    /// Calls native operations with the partly constructed array visible to the collector.
    pub(in crate::interpreter) fn with_values<R>(&mut self, use_values: impl FnOnce(&mut V) -> R) -> R {
        with_runtime_owners(&self.cell, || use_values(self.values))
    }

    /// Stages temporary cells for one conditional insertion while keeping the result rooted.
    pub(in crate::interpreter) fn update(
        &mut self,
        prepare: impl FnOnce(&mut V, &StagedRuntimeOwners, RuntimeCellHandle)
            -> Result<Option<(RuntimeCellHandle, RuntimeCellHandle)>, EvalStatus>,
    ) -> Result<(), EvalStatus> {
        let result = &self.cell;
        let values = &mut *self.values;
        with_runtime_owners(result, || {
            with_collection_cells(values, |values, owners| {
                if let Some((key, value)) = prepare(values, owners, result.borrowed())? {
                    result.update_receiver(values.array_set(result.borrowed(), key, value)?);
                }
                Ok(())
            })
        })
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
        self.stage_entry(|values, key_owner, value_owner| {
            let value = value(values)?;
            value_owner.replace(Some(value));
            let key = key(values, value)?;
            key_owner.replace(Some(key));
            Ok((key, value))
        })
    }

    /// Boxes an independent key before evaluating a potentially nested value.
    pub(in crate::interpreter) fn entry_key_first(
        &mut self,
        key: impl FnOnce(&mut V) -> Result<RuntimeCellHandle, EvalStatus>,
        value: impl FnOnce(&mut V) -> Result<RuntimeCellHandle, EvalStatus>,
    ) -> Result<(), EvalStatus> {
        self.entry_from_key(key, |values, _| value(values))
    }

    /// Materializes a value using the rooted entry key.
    pub(in crate::interpreter) fn entry_from_key(
        &mut self,
        key: impl FnOnce(&mut V) -> Result<RuntimeCellHandle, EvalStatus>,
        value: impl FnOnce(&mut V, RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>,
    ) -> Result<(), EvalStatus> {
        self.stage_entry(|values, key_owner, value_owner| {
            let key = key(values)?;
            key_owner.replace(Some(key));
            let value = value(values, key)?;
            value_owner.replace(Some(value));
            Ok((key, value))
        })
    }

    /// Inserts the acquired entry and retires its operands on every outcome.
    fn stage_entry(
        &mut self,
        acquire: impl FnOnce(&mut V, &RuntimeOwnerSlot, &RuntimeOwnerSlot)
            -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus>,
    ) -> Result<(), EvalStatus> {
        let key_owner = RuntimeOwnerSlot::empty();
        let value_owner = RuntimeOwnerSlot::empty();
        with_runtime_owners(&(&self.cell, (&key_owner, &value_owner)), || {
            let inserted = (|| {
                let (key, value) = acquire(self.values, &key_owner, &value_owner)?;
                let array = self.values.array_set(self.cell.borrowed(), key, value)?;
                // The setter returns the same receiver lease after any payload relocation.
                self.cell.update_receiver(array);
                Ok(())
            })();
            let cells = [&key_owner, &value_owner].into_iter().map(RuntimeOwnerSlot::take);
            release_staged_cells(inserted, cells, |cell| self.values.release(cell))
        })
    }

    /// Constructs and appends an object while the partially populated result stays rooted.
    pub(in crate::interpreter) fn object(
        &mut self,
        construct: impl FnOnce(&mut V) -> Result<RuntimeCellHandle, EvalStatus>,
    ) -> Result<(), EvalStatus> {
        let owner = RuntimeOwnerSlot::empty();
        with_runtime_owners(&(&self.cell, &owner), || {
            let object = construct(self.values)?;
            owner.replace(Some(object));
            let appended = self.values.object_array_push(self.cell.borrowed(), object).map(|updated| {
                self.cell.update_receiver(updated);
            });
            release_staged_cells(appended, [owner.take()], |cell| self.values.release(cell))
        })
    }

    /// Abandons the builder and returns the release status to the caller.
    pub(in crate::interpreter) fn discard(self) -> Result<(), EvalStatus> {
        let cell = self.cell.take();
        self.values.release(cell)
    }

    /// Transfers the finished array to the caller without releasing its entries.
    pub(in crate::interpreter) fn finish(self) -> RuntimeCellHandle {
        self.cell.take()
    }
}

/// Runs a collection step and drains all its temporary owners, including on error.
/// Any returned runtime cell must be held by a separate rooted result slot.
pub(in crate::interpreter) fn with_collection_cells<V: RuntimeValueOps, R>(
    values: &mut V,
    operation: impl FnOnce(&mut V, &StagedRuntimeOwners) -> Result<R, EvalStatus>,
) -> Result<R, EvalStatus> {
    let owners = StagedRuntimeOwners::default();
    with_runtime_owners(&owners, || {
        let result = operation(values, &owners);
        owners.complete(result, |cell| values.release(cell))
    })
}

/// Acquires an iterator key and its element, registering each before the next runtime call.
pub(in crate::interpreter) fn stage_collection_entry(
    array: RuntimeCellHandle,
    position: usize,
    values: &mut impl RuntimeValueOps,
    owners: &StagedRuntimeOwners,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let key = owners.stage(values.array_iter_key(array, position)?);
    let value = owners.stage(values.array_get(array, key.borrowed())?);
    Ok((key.borrowed(), value.borrowed()))
}

/// Builds or updates a result while keeping it visible through each step's cleanup.
pub(in crate::interpreter) fn with_collection_result<V: RuntimeValueOps>(
    values: &mut V,
    operation: impl FnOnce(&mut V, &RuntimeOwnerSlot) -> Result<(), EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = RuntimeOwnerSlot::empty();
    with_runtime_owners(&result, || {
        let outcome = operation(values, &result);
        result.finish(outcome, |cell| values.release(cell))
    })
}

/// Roots a direct eval call's operands and result through source evaluation and writeback.
pub(in crate::interpreter) fn with_collection_call<V: RuntimeValueOps>(
    context: &mut ElephcEvalContext,
    values: &mut V,
    operation: impl FnOnce(
        &mut ElephcEvalContext, &mut V, &StagedRuntimeOwners, &RuntimeOwnerSlot,
    ) -> Result<(), EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let owners = StagedRuntimeOwners::default();
    let result = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&owners, &result), || {
        let outcome = operation(context, values, &owners, &result);
        owners.finish(outcome, &result, |cell| release_expr_result(cell, context, values))
    })
}

impl<V: RuntimeValueOps> Drop for EvalArrayBuilder<'_, V> {
    /// Releases unfinished arrays on early returns, including nested builder failures.
    fn drop(&mut self) {
        if let Some(cell) = self.cell.take_optional() {
            let _ = self.values.release(cell);
        }
    }
}
