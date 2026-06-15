//! Purpose:
//! Bridges EvalIR value operations to elephc runtime values.
//! Calls C-ABI wrapper symbols emitted by the main runtime object when eval is
//! enabled, avoiding a duplicate PHP value representation inside this crate.
//!
//! Called from:
//! - `crate::__elephc_eval_execute()` in non-test builds.
//!
//! Key details:
//! - The wrapper symbols adapt to elephc's target-specific internal helper ABI.
//! - Unit tests do not link the generated runtime object, so this module's real
//!   hook implementation is compiled only outside `cfg(test)`.

#[cfg(not(test))]
use crate::errors::EvalStatus;
#[cfg(not(test))]
use crate::interpreter::RuntimeValueOps;
#[cfg(not(test))]
use crate::value::{RuntimeCell, RuntimeCellHandle};

#[cfg(not(test))]
unsafe extern "C" {
    fn __elephc_eval_value_array_new(capacity: u64) -> *mut RuntimeCell;
    fn __elephc_eval_value_assoc_new(capacity: u64) -> *mut RuntimeCell;
    fn __elephc_eval_value_array_get(
        array: *mut RuntimeCell,
        index: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_array_set(
        array: *mut RuntimeCell,
        index: *mut RuntimeCell,
        value: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_is_array_like(value: *mut RuntimeCell) -> u64;
    fn __elephc_eval_value_null() -> *mut RuntimeCell;
    fn __elephc_eval_value_bool(value: u64) -> *mut RuntimeCell;
    fn __elephc_eval_value_int(value: i64) -> *mut RuntimeCell;
    fn __elephc_eval_value_float(value: f64) -> *mut RuntimeCell;
    fn __elephc_eval_value_string(ptr: *const u8, len: u64) -> *mut RuntimeCell;
    fn __elephc_eval_value_add(left: *mut RuntimeCell, right: *mut RuntimeCell)
        -> *mut RuntimeCell;
    fn __elephc_eval_value_sub(left: *mut RuntimeCell, right: *mut RuntimeCell)
        -> *mut RuntimeCell;
    fn __elephc_eval_value_mul(left: *mut RuntimeCell, right: *mut RuntimeCell)
        -> *mut RuntimeCell;
    fn __elephc_eval_value_concat(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_echo(value: *mut RuntimeCell);
    fn __elephc_eval_value_truthy(value: *mut RuntimeCell) -> u64;
    fn __elephc_eval_value_release(value: *mut RuntimeCell);
}

/// Runtime hook adapter that produces and consumes boxed elephc Mixed cells.
#[cfg(not(test))]
pub struct ElephcRuntimeOps;

#[cfg(not(test))]
impl ElephcRuntimeOps {
    /// Creates a new stateless runtime hook adapter.
    pub const fn new() -> Self {
        Self
    }

    /// Converts a runtime wrapper result into an interpreter handle.
    fn handle(ptr: *mut RuntimeCell) -> Result<RuntimeCellHandle, EvalStatus> {
        if ptr.is_null() {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(RuntimeCellHandle::from_raw(ptr))
        }
    }
}

#[cfg(not(test))]
impl RuntimeValueOps for ElephcRuntimeOps {
    /// Creates a boxed Mixed indexed array through the generated runtime wrapper.
    fn array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_array_new(capacity as u64) })
    }

    /// Creates a boxed Mixed associative array through the generated runtime wrapper.
    fn assoc_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_assoc_new(capacity as u64) })
    }

    /// Reads one element from a boxed Mixed array through the generated runtime wrapper.
    fn array_get(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_array_get(array.as_ptr(), index.as_ptr()) })
    }

    /// Writes one element to a boxed Mixed array through the generated runtime wrapper.
    fn array_set(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_value_array_set(array.as_ptr(), index.as_ptr(), value.as_ptr())
        })
    }

    /// Returns whether a boxed Mixed cell has an array-like runtime tag.
    fn is_array_like(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_value_is_array_like(value.as_ptr()) != 0 })
    }

    /// Releases one boxed Mixed cell through the generated runtime wrapper.
    fn release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        unsafe {
            __elephc_eval_value_release(value.as_ptr());
        }
        Ok(())
    }

    /// Creates a boxed null Mixed cell through the generated runtime wrapper.
    fn null(&mut self) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_null() })
    }

    /// Creates a boxed bool Mixed cell through the generated runtime wrapper.
    fn bool_value(&mut self, value: bool) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_bool(u64::from(value)) })
    }

    /// Creates a boxed int Mixed cell through the generated runtime wrapper.
    fn int(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_int(value) })
    }

    /// Creates a boxed float Mixed cell through the generated runtime wrapper.
    fn float(&mut self, value: f64) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_float(value) })
    }

    /// Creates a boxed string Mixed cell through the generated runtime wrapper.
    fn string(&mut self, value: &str) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_string(value.as_ptr(), value.len() as u64) })
    }

    /// Adds two boxed Mixed cells using elephc runtime numeric semantics.
    fn add(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_add(left.as_ptr(), right.as_ptr()) })
    }

    /// Subtracts two boxed Mixed cells using elephc runtime numeric semantics.
    fn sub(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_sub(left.as_ptr(), right.as_ptr()) })
    }

    /// Multiplies two boxed Mixed cells using elephc runtime numeric semantics.
    fn mul(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_mul(left.as_ptr(), right.as_ptr()) })
    }

    /// Concatenates two boxed Mixed cells using elephc runtime string semantics.
    fn concat(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_concat(left.as_ptr(), right.as_ptr()) })
    }

    /// Emits one boxed Mixed cell to stdout through the generated runtime wrapper.
    fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        unsafe {
            __elephc_eval_value_echo(value.as_ptr());
        }
        Ok(())
    }

    /// Converts one boxed Mixed cell to PHP truthiness through the generated runtime wrapper.
    fn truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_value_truthy(value.as_ptr()) != 0 })
    }
}
