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
use crate::eval_ir::EvalBinOp;
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
    fn __elephc_eval_value_property_get(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_property_set(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        value: *mut RuntimeCell,
    ) -> u64;
    fn __elephc_eval_value_method_call(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        args: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_array_len(array: *mut RuntimeCell) -> u64;
    fn __elephc_eval_value_is_array_like(value: *mut RuntimeCell) -> u64;
    fn __elephc_eval_value_is_null(value: *mut RuntimeCell) -> u64;
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
    fn __elephc_eval_value_compare(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
        op: u64,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_echo(value: *mut RuntimeCell);
    fn __elephc_eval_value_string_bytes(
        value: *mut RuntimeCell,
        out_ptr: *mut *const u8,
        out_len: *mut u64,
    ) -> u64;
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

    /// Reads a boxed Mixed object property through the generated user helper.
    fn property_get(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_value_property_get(
                object.as_ptr(),
                property.as_ptr(),
                property.len() as u64,
            )
        })
    }

    /// Writes a boxed Mixed object property through the generated user helper.
    fn property_set(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
        value: RuntimeCellHandle,
    ) -> Result<(), EvalStatus> {
        let ok = unsafe {
            __elephc_eval_value_property_set(
                object.as_ptr(),
                property.as_ptr(),
                property.len() as u64,
                value.as_ptr(),
            )
        };
        if ok == 0 {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(())
        }
    }

    /// Calls a boxed Mixed object method through the generated user helper.
    fn method_call(
        &mut self,
        object: RuntimeCellHandle,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let arg_array = unsafe { __elephc_eval_value_array_new(args.len() as u64) };
        let arg_array = Self::handle(arg_array)?;
        for (index, value) in args.into_iter().enumerate() {
            let index = Self::handle(unsafe { __elephc_eval_value_int(index as i64) })?;
            Self::handle(unsafe {
                __elephc_eval_value_array_set(arg_array.as_ptr(), index.as_ptr(), value.as_ptr())
            })?;
        }
        let result = Self::handle(unsafe {
            __elephc_eval_value_method_call(
                object.as_ptr(),
                method.as_ptr(),
                method.len() as u64,
                arg_array.as_ptr(),
            )
        });
        unsafe {
            __elephc_eval_value_release(arg_array.as_ptr());
        }
        result
    }

    /// Returns the visible element count for a boxed Mixed array through the generated runtime wrapper.
    fn array_len(&mut self, array: RuntimeCellHandle) -> Result<usize, EvalStatus> {
        let len = unsafe { __elephc_eval_value_array_len(array.as_ptr()) };
        usize::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)
    }

    /// Returns whether a boxed Mixed cell has an array-like runtime tag.
    fn is_array_like(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_value_is_array_like(value.as_ptr()) != 0 })
    }

    /// Returns whether a boxed Mixed cell unwraps to PHP null.
    fn is_null(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_value_is_null(value.as_ptr()) != 0 })
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

    /// Compares two boxed Mixed cells through the generated runtime wrapper.
    fn compare(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_value_compare(left.as_ptr(), right.as_ptr(), compare_op_tag(op))
        })
    }

    /// Emits one boxed Mixed cell to stdout through the generated runtime wrapper.
    fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        unsafe {
            __elephc_eval_value_echo(value.as_ptr());
        }
        Ok(())
    }

    /// Casts one boxed Mixed cell to a PHP string and copies the bytes into Rust memory.
    fn string_bytes(&mut self, value: RuntimeCellHandle) -> Result<Vec<u8>, EvalStatus> {
        let mut ptr = std::ptr::null();
        let mut len = 0;
        let ok = unsafe { __elephc_eval_value_string_bytes(value.as_ptr(), &mut ptr, &mut len) };
        if ok == 0 || (len > 0 && ptr.is_null()) {
            return Err(EvalStatus::RuntimeFatal);
        }
        let len = usize::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
        let bytes = if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(ptr, len) }
        };
        Ok(bytes.to_vec())
    }

    /// Converts one boxed Mixed cell to PHP truthiness through the generated runtime wrapper.
    fn truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_value_truthy(value.as_ptr()) != 0 })
    }
}

/// Maps an EvalIR comparison operator to the bridge ABI opcode.
#[cfg(not(test))]
fn compare_op_tag(op: EvalBinOp) -> u64 {
    match op {
        EvalBinOp::LooseEq => 0,
        EvalBinOp::LooseNotEq => 1,
        EvalBinOp::Lt => 2,
        EvalBinOp::LtEq => 3,
        EvalBinOp::Gt => 4,
        EvalBinOp::GtEq => 5,
        EvalBinOp::StrictEq => 6,
        EvalBinOp::StrictNotEq => 7,
        EvalBinOp::Add
        | EvalBinOp::Sub
        | EvalBinOp::Mul
        | EvalBinOp::Concat
        | EvalBinOp::LogicalAnd
        | EvalBinOp::LogicalOr => 0,
    }
}
