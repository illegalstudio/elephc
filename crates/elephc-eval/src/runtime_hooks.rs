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
    fn __elephc_eval_value_array_key_exists(
        key: *mut RuntimeCell,
        array: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_array_iter_key(
        array: *mut RuntimeCell,
        position: u64,
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
    fn __elephc_eval_value_object_property_len(object: *mut RuntimeCell) -> u64;
    fn __elephc_eval_value_object_property_iter_key(
        object: *mut RuntimeCell,
        position: u64,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_method_call(
        object: *mut RuntimeCell,
        name_ptr: *const u8,
        name_len: u64,
        args: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_new_object(name_ptr: *const u8, name_len: u64) -> *mut RuntimeCell;
    fn __elephc_eval_value_construct_object(
        object: *mut RuntimeCell,
        args: *mut RuntimeCell,
    ) -> u64;
    fn __elephc_eval_class_exists(name_ptr: *const u8, name_len: u64) -> u64;
    fn __elephc_eval_value_array_len(array: *mut RuntimeCell) -> u64;
    fn __elephc_eval_value_is_array_like(value: *mut RuntimeCell) -> u64;
    fn __elephc_eval_value_is_null(value: *mut RuntimeCell) -> u64;
    fn __elephc_eval_value_type_tag(value: *mut RuntimeCell) -> u64;
    fn __elephc_eval_warning(message_ptr: *const u8, message_len: u64);
    fn __elephc_eval_value_null() -> *mut RuntimeCell;
    fn __elephc_eval_value_bool(value: u64) -> *mut RuntimeCell;
    fn __elephc_eval_value_int(value: i64) -> *mut RuntimeCell;
    fn __elephc_eval_value_float(value: f64) -> *mut RuntimeCell;
    fn __elephc_eval_value_string(ptr: *const u8, len: u64) -> *mut RuntimeCell;
    fn __elephc_eval_value_cast_int(value: *mut RuntimeCell) -> *mut RuntimeCell;
    fn __elephc_eval_value_cast_float(value: *mut RuntimeCell) -> *mut RuntimeCell;
    fn __elephc_eval_value_cast_string(value: *mut RuntimeCell) -> *mut RuntimeCell;
    fn __elephc_eval_value_cast_bool(value: *mut RuntimeCell) -> *mut RuntimeCell;
    fn __elephc_eval_value_abs(value: *mut RuntimeCell) -> *mut RuntimeCell;
    fn __elephc_eval_value_ceil(value: *mut RuntimeCell) -> *mut RuntimeCell;
    fn __elephc_eval_value_floor(value: *mut RuntimeCell) -> *mut RuntimeCell;
    fn __elephc_eval_value_sqrt(value: *mut RuntimeCell) -> *mut RuntimeCell;
    fn __elephc_eval_value_strrev(value: *mut RuntimeCell) -> *mut RuntimeCell;
    fn __elephc_eval_value_fdiv(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_fmod(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_add(left: *mut RuntimeCell, right: *mut RuntimeCell)
        -> *mut RuntimeCell;
    fn __elephc_eval_value_sub(left: *mut RuntimeCell, right: *mut RuntimeCell)
        -> *mut RuntimeCell;
    fn __elephc_eval_value_mul(left: *mut RuntimeCell, right: *mut RuntimeCell)
        -> *mut RuntimeCell;
    fn __elephc_eval_value_div(left: *mut RuntimeCell, right: *mut RuntimeCell)
        -> *mut RuntimeCell;
    fn __elephc_eval_value_mod(left: *mut RuntimeCell, right: *mut RuntimeCell)
        -> *mut RuntimeCell;
    fn __elephc_eval_value_pow(left: *mut RuntimeCell, right: *mut RuntimeCell)
        -> *mut RuntimeCell;
    fn __elephc_eval_value_round(
        value: *mut RuntimeCell,
        precision: *mut RuntimeCell,
        has_precision: u64,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_bitwise(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
        op: u64,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_bit_not(value: *mut RuntimeCell) -> *mut RuntimeCell;
    fn __elephc_eval_value_concat(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_compare(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
        op: u64,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_spaceship(
        left: *mut RuntimeCell,
        right: *mut RuntimeCell,
    ) -> *mut RuntimeCell;
    fn __elephc_eval_value_echo(value: *mut RuntimeCell);
    fn __elephc_eval_value_string_bytes(
        value: *mut RuntimeCell,
        out_ptr: *mut *const u8,
        out_len: *mut u64,
    ) -> u64;
    fn __elephc_eval_value_truthy(value: *mut RuntimeCell) -> u64;
    fn __elephc_eval_value_release(value: *mut RuntimeCell);
    fn __elephc_eval_value_retain(value: *mut RuntimeCell) -> *mut RuntimeCell;
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

    /// Packs source-order argument cells into the boxed eval array ABI.
    fn arg_array(args: Vec<RuntimeCellHandle>) -> Result<RuntimeCellHandle, EvalStatus> {
        let arg_array = unsafe { __elephc_eval_value_array_new(args.len() as u64) };
        let arg_array = Self::handle(arg_array)?;
        for (index, value) in args.into_iter().enumerate() {
            let index = Self::handle(unsafe { __elephc_eval_value_int(index as i64) })?;
            Self::handle(unsafe {
                __elephc_eval_value_array_set(arg_array.as_ptr(), index.as_ptr(), value.as_ptr())
            })?;
        }
        Ok(arg_array)
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

    /// Checks whether a boxed Mixed array contains a normalized PHP key.
    fn array_key_exists(
        &mut self,
        key: RuntimeCellHandle,
        array: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_array_key_exists(key.as_ptr(), array.as_ptr()) })
    }

    /// Returns one foreach-visible key from a boxed Mixed array by iteration position.
    fn array_iter_key(
        &mut self,
        array: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_array_iter_key(array.as_ptr(), position as u64) })
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

    /// Returns the JSON-visible public property count for a boxed Mixed object.
    fn object_property_len(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<usize, EvalStatus> {
        let len = unsafe { __elephc_eval_value_object_property_len(object.as_ptr()) };
        usize::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)
    }

    /// Returns one JSON-visible public property key for a boxed Mixed object.
    fn object_property_iter_key(
        &mut self,
        object: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_value_object_property_iter_key(object.as_ptr(), position as u64)
        })
    }

    /// Calls a boxed Mixed object method through the generated user helper.
    fn method_call(
        &mut self,
        object: RuntimeCellHandle,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let arg_array = Self::arg_array(args)?;
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

    /// Creates a boxed Mixed object through the generated dynamic class-name wrapper.
    fn new_object(&mut self, class_name: &str) -> Result<RuntimeCellHandle, EvalStatus> {
        let object = Self::handle(unsafe {
            __elephc_eval_value_new_object(class_name.as_ptr(), class_name.len() as u64)
        })?;
        match self.is_null(object) {
            Ok(false) => Ok(object),
            Ok(true) => {
                self.release(object)?;
                Err(EvalStatus::RuntimeFatal)
            }
            Err(err) => {
                let _ = self.release(object);
                Err(err)
            }
        }
    }

    /// Calls a public AOT constructor through the generated user bridge when one exists.
    fn construct_object(
        &mut self,
        object: RuntimeCellHandle,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<(), EvalStatus> {
        let arg_array = Self::arg_array(args)?;
        let ok =
            unsafe { __elephc_eval_value_construct_object(object.as_ptr(), arg_array.as_ptr()) };
        unsafe {
            __elephc_eval_value_release(arg_array.as_ptr());
        }
        if ok == 0 {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(())
        }
    }

    /// Returns whether the generated AOT class-name table contains the requested class.
    fn class_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_class_exists(name.as_ptr(), name.len() as u64) != 0 })
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

    /// Returns the unboxed Mixed runtime tag for PHP type-predicate builtins.
    fn type_tag(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        Ok(unsafe { __elephc_eval_value_type_tag(value.as_ptr()) })
    }

    /// Releases one boxed Mixed cell through the generated runtime wrapper.
    fn release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
        unsafe {
            __elephc_eval_value_release(value.as_ptr());
        }
        Ok(())
    }

    /// Retains one boxed Mixed cell through the generated runtime wrapper.
    fn retain(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(RuntimeCellHandle::from_raw(unsafe {
            __elephc_eval_value_retain(value.as_ptr())
        }))
    }

    /// Emits one PHP warning through the generated runtime diagnostic helper.
    fn warning(&mut self, message: &str) -> Result<(), EvalStatus> {
        unsafe {
            __elephc_eval_warning(message.as_ptr(), message.len() as u64);
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

    /// Creates a boxed string Mixed cell from raw PHP bytes through the generated runtime wrapper.
    fn string_bytes_value(&mut self, value: &[u8]) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_string(value.as_ptr(), value.len() as u64) })
    }

    /// Casts a boxed Mixed cell to a boxed integer Mixed cell through the generated runtime wrapper.
    fn cast_int(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_cast_int(value.as_ptr()) })
    }

    /// Casts a boxed Mixed cell to a boxed float Mixed cell through the generated runtime wrapper.
    fn cast_float(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_cast_float(value.as_ptr()) })
    }

    /// Casts a boxed Mixed cell to a boxed string Mixed cell through the generated runtime wrapper.
    fn cast_string(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_cast_string(value.as_ptr()) })
    }

    /// Casts a boxed Mixed cell to a boxed boolean Mixed cell through the generated runtime wrapper.
    fn cast_bool(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_cast_bool(value.as_ptr()) })
    }

    /// Computes PHP `abs()` for a boxed Mixed cell through the generated runtime wrapper.
    fn abs(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_abs(value.as_ptr()) })
    }

    /// Computes PHP `ceil()` for a boxed Mixed cell through the generated runtime wrapper.
    fn ceil(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_ceil(value.as_ptr()) })
    }

    /// Computes PHP `floor()` for a boxed Mixed cell through the generated runtime wrapper.
    fn floor(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_floor(value.as_ptr()) })
    }

    /// Computes PHP `sqrt()` for a boxed Mixed cell through the generated runtime wrapper.
    fn sqrt(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_sqrt(value.as_ptr()) })
    }

    /// Computes PHP `strrev()` for a boxed Mixed cell through the generated runtime wrapper.
    fn strrev(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_strrev(value.as_ptr()) })
    }

    /// Computes PHP `fdiv()` for boxed Mixed cells through the generated runtime wrapper.
    fn fdiv(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_fdiv(left.as_ptr(), right.as_ptr()) })
    }

    /// Computes PHP `fmod()` for boxed Mixed cells through the generated runtime wrapper.
    fn fmod(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_fmod(left.as_ptr(), right.as_ptr()) })
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

    /// Divides two boxed Mixed cells using elephc runtime numeric semantics.
    fn div(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_div(left.as_ptr(), right.as_ptr()) })
    }

    /// Computes modulo for two boxed Mixed cells using elephc runtime integer semantics.
    fn modulo(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_mod(left.as_ptr(), right.as_ptr()) })
    }

    /// Raises two boxed Mixed cells using elephc runtime numeric exponentiation semantics.
    fn pow(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_pow(left.as_ptr(), right.as_ptr()) })
    }

    /// Rounds a boxed Mixed cell through the generated runtime wrapper.
    fn round(
        &mut self,
        value: RuntimeCellHandle,
        precision: Option<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let (precision, has_precision) = if let Some(precision) = precision {
            (precision.as_ptr(), 1)
        } else {
            (core::ptr::null_mut(), 0)
        };
        Self::handle(unsafe { __elephc_eval_value_round(value.as_ptr(), precision, has_precision) })
    }

    /// Applies an integer bitwise or shift operation through the generated runtime wrapper.
    fn bitwise(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_value_bitwise(left.as_ptr(), right.as_ptr(), bitwise_op_tag(op))
        })
    }

    /// Applies integer bitwise NOT through the generated runtime wrapper.
    fn bit_not(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_bit_not(value.as_ptr()) })
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

    /// Computes a PHP numeric spaceship result through the generated runtime wrapper.
    fn spaceship(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_spaceship(left.as_ptr(), right.as_ptr()) })
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
        | EvalBinOp::Div
        | EvalBinOp::Mod
        | EvalBinOp::Pow
        | EvalBinOp::BitAnd
        | EvalBinOp::BitOr
        | EvalBinOp::BitXor
        | EvalBinOp::ShiftLeft
        | EvalBinOp::ShiftRight
        | EvalBinOp::Concat
        | EvalBinOp::Spaceship
        | EvalBinOp::LogicalAnd
        | EvalBinOp::LogicalOr
        | EvalBinOp::LogicalXor => 0,
    }
}

/// Maps bitwise EvalIR operators onto the generated runtime wrapper opcode table.
#[cfg(not(test))]
fn bitwise_op_tag(op: EvalBinOp) -> u64 {
    match op {
        EvalBinOp::BitAnd => 0,
        EvalBinOp::BitOr => 1,
        EvalBinOp::BitXor => 2,
        EvalBinOp::ShiftLeft => 3,
        EvalBinOp::ShiftRight => 4,
        EvalBinOp::Add
        | EvalBinOp::Sub
        | EvalBinOp::Mul
        | EvalBinOp::Div
        | EvalBinOp::Mod
        | EvalBinOp::Pow
        | EvalBinOp::Concat
        | EvalBinOp::LogicalAnd
        | EvalBinOp::LogicalOr
        | EvalBinOp::LogicalXor
        | EvalBinOp::LooseEq
        | EvalBinOp::LooseNotEq
        | EvalBinOp::StrictEq
        | EvalBinOp::StrictNotEq
        | EvalBinOp::Lt
        | EvalBinOp::LtEq
        | EvalBinOp::Gt
        | EvalBinOp::GtEq
        | EvalBinOp::Spaceship => 0,
    }
}
