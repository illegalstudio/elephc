//! Purpose:
//! Defines the array, property, object-call, static-call, and native-result
//! methods of the generated-runtime `RuntimeValueOps` adapter.
//!
//! Called from:
//! - The single `RuntimeValueOps for ElephcRuntimeOps` implementation in `super`.
//!
//! Key details:
//! - Temporary argument arrays are released after bridge calls.

macro_rules! impl_collection_call_ops {
    () => {
    /// Creates a boxed Mixed indexed array through the generated runtime wrapper.
    fn array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_array_new(capacity as u64) })
    }

    /// Creates a boxed Mixed indexed array whose payload uses direct string slots.
    fn string_array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_string_array_new(capacity as u64) })
    }

    /// Appends one string to a boxed direct-string indexed array.
    fn string_array_push(
        &mut self,
        array: RuntimeCellHandle,
        value: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_value_string_array_push(
                array.as_ptr(),
                value.as_ptr(),
                value.len() as u64,
            )
        })
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
        let (scope_ptr, scope_len) = self.current_class_scope_abi();
        Self::handle(unsafe {
            __elephc_eval_value_property_get(
                object.as_ptr(),
                property.as_ptr(),
                property.len() as u64,
                scope_ptr,
                scope_len,
            )
        })
    }

    /// Checks an AOT instance property initialization marker through the generated helper.
    fn property_is_initialized(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
    ) -> Result<bool, EvalStatus> {
        let (scope_ptr, scope_len) = self.current_class_scope_abi();
        let initialized = unsafe {
            __elephc_eval_value_property_is_initialized(
                object.as_ptr(),
                property.as_ptr(),
                property.len() as u64,
                scope_ptr,
                scope_len,
            )
        };
        Ok(initialized != 0)
    }

    /// Writes a boxed Mixed object property through the generated user helper.
    fn property_set(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
        value: RuntimeCellHandle,
    ) -> Result<(), EvalStatus> {
        let (scope_ptr, scope_len) = self.current_class_scope_abi();
        let ok = unsafe {
            __elephc_eval_value_property_set(
                object.as_ptr(),
                property.as_ptr(),
                property.len() as u64,
                value.as_ptr(),
                scope_ptr,
                scope_len,
            )
        };
        if ok == 0 {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(())
        }
    }

    /// Reads an AOT static property through the generated user helper.
    fn static_property_get(
        &mut self,
        class_name: &str,
        property: &str,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let (scope_ptr, scope_len) = self.current_class_scope_abi();
        let ptr = unsafe {
            __elephc_eval_value_static_property_get(
                class_name.as_ptr(),
                class_name.len() as u64,
                property.as_ptr(),
                property.len() as u64,
                scope_ptr,
                scope_len,
            )
        };
        if ptr.is_null() {
            Ok(None)
        } else {
            Ok(Some(RuntimeCellHandle::from_raw(ptr)))
        }
    }

    /// Checks an AOT static property initialization marker through the generated helper.
    fn static_property_is_initialized(
        &mut self,
        class_name: &str,
        property: &str,
    ) -> Result<bool, EvalStatus> {
        let (scope_ptr, scope_len) = self.current_class_scope_abi();
        let initialized = unsafe {
            __elephc_eval_value_static_property_is_initialized(
                class_name.as_ptr(),
                class_name.len() as u64,
                property.as_ptr(),
                property.len() as u64,
                scope_ptr,
                scope_len,
            )
        };
        Ok(initialized != 0)
    }

    /// Writes an AOT static property through the generated user helper.
    fn static_property_set(
        &mut self,
        class_name: &str,
        property: &str,
        value: RuntimeCellHandle,
    ) -> Result<bool, EvalStatus> {
        let (scope_ptr, scope_len) = self.current_class_scope_abi();
        let ok = unsafe {
            __elephc_eval_value_static_property_set(
                class_name.as_ptr(),
                class_name.len() as u64,
                property.as_ptr(),
                property.len() as u64,
                value.as_ptr(),
                scope_ptr,
                scope_len,
            )
        };
        Ok(ok != 0)
    }

    /// Reads an AOT class-like constant through the generated user helper.
    fn class_constant_get(
        &mut self,
        class_name: &str,
        constant: &str,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let (scope_ptr, scope_len) = self.current_class_scope_abi();
        let ptr = unsafe {
            __elephc_eval_value_class_constant_get(
                class_name.as_ptr(),
                class_name.len() as u64,
                constant.as_ptr(),
                constant.len() as u64,
                scope_ptr,
                scope_len,
            )
        };
        if ptr.is_null() {
            Ok(None)
        } else {
            Ok(Some(RuntimeCellHandle::from_raw(ptr)))
        }
    }

    /// Creates a shallow clone of a boxed Mixed stdClass/eval object through the generated wrapper.
    fn object_clone_shallow(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_object_clone_shallow(object.as_ptr()) })
    }

    /// Returns the JSON-visible public property count for a boxed Mixed object.
    fn object_property_len(&mut self, object: RuntimeCellHandle) -> Result<usize, EvalStatus> {
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
        let (scope_ptr, scope_len) = self.current_class_scope_abi();
        let arg_array = Self::arg_array(args)?;
        let result = unsafe {
            __elephc_eval_value_method_call(
                object.as_ptr(),
                method.as_ptr(),
                method.len() as u64,
                arg_array.as_ptr(),
                scope_ptr,
                scope_len,
                self.context.cast(),
            )
        };
        unsafe {
            __elephc_eval_value_release(arg_array.as_ptr());
        }
        self.handle_native_call_result(result)
    }

    /// Calls an AOT static method through the generated user helper.
    fn static_method_call(
        &mut self,
        class_name: &str,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let (scope_ptr, scope_len) = self.current_class_scope_abi();
        let arg_array = Self::arg_array(args)?;
        let result = unsafe {
            __elephc_eval_value_static_method_call(
                class_name.as_ptr(),
                class_name.len() as u64,
                method.as_ptr(),
                method.len() as u64,
                arg_array.as_ptr(),
                scope_ptr,
                scope_len,
                self.context.cast(),
            )
        };
        unsafe {
            __elephc_eval_value_release(arg_array.as_ptr());
        }
        self.handle_native_call_result(result)
    }

    /// Converts a native free-function result into eval status, preserving pending throwables.
    fn native_call_result(
        &mut self,
        result: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.handle_native_call_result(result.as_ptr())
    }

    };
}

pub(super) use impl_collection_call_ops;
