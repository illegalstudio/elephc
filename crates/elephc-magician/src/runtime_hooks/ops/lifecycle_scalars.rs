//! Purpose:
//! Defines object identity, retain/release, warning, scalar construction, and
//! scalar-cast methods for the generated runtime adapter.
//!
//! Called from:
//! - The single `RuntimeValueOps for ElephcRuntimeOps` implementation in `super`.
//!
//! Key details:
//! - Every runtime pointer is validated before it becomes a handle.

macro_rules! impl_lifecycle_scalar_ops {
    () => {

    /// Returns the unboxed object payload pointer for SPL object identity builtins.
    fn object_identity(&mut self, object: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        let identity = unsafe { __elephc_eval_value_object_identity(object.as_ptr()) };
        if identity == 0 {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(identity)
        }
    }

    /// Returns the object payload that the next release would destroy, when known.
    fn final_object_identity_for_release(
        &mut self,
        value: RuntimeCellHandle,
    ) -> Result<Option<u64>, EvalStatus> {
        let identity = unsafe { __elephc_eval_value_final_object_identity(value.as_ptr()) };
        Ok((identity != 0).then_some(identity))
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

    /// Creates a boxed resource Mixed cell through the generated runtime wrapper.
    fn resource(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_resource(value) })
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

    };
}

pub(super) use impl_lifecycle_scalar_ops;
