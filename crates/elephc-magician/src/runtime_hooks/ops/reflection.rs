//! Purpose:
//! Defines reflection object materialization and metadata-query methods for the
//! generated-runtime `RuntimeValueOps` adapter.
//!
//! Called from:
//! - The single `RuntimeValueOps for ElephcRuntimeOps` implementation in `super`.
//!
//! Key details:
//! - Nullable wrapper results are mapped to `Option` without manufacturing handles.

macro_rules! impl_reflection_ops {
    () => {

    /// Boxes a native default factory independently of the lifetime of the registering eval call.
    fn native_callable_from_raw(&mut self, descriptor: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_value_callable_from_raw(descriptor)
        })
    }

    /// Decodes trusted static descriptor metadata without exposing compiler-internal parameters.
    fn native_callable_signature(
        &mut self,
        callback: RuntimeCellHandle,
    ) -> Result<Option<NativeCallableSignatureMetadata>, EvalStatus> {
        const MAX_DESCRIPTOR_PARAMETERS: usize = 4096;

        let descriptor = self.raw_value_word(callback)?;
        let mut signature = CallableSignatureViewV1::default();
        match unsafe { __elephc_eval_callable_signature_v1(descriptor, &mut signature) } {
            0 => return Ok(None),
            1 => {}
            _ => return Err(EvalStatus::RuntimeFatal),
        }
        let param_count = signature.parameter_count;
        let regular_param_count = signature.regular_count;
        if param_count > MAX_DESCRIPTOR_PARAMETERS || regular_param_count > param_count {
            return Err(EvalStatus::RuntimeFatal);
        }
        let variadic_index = if signature.variadic == NO_VARIADIC_PARAMETER {
            None
        } else {
            let index = usize::try_from(signature.variadic).map_err(|_| EvalStatus::RuntimeFatal)?;
            if index >= param_count || index != regular_param_count {
                return Err(EvalStatus::RuntimeFatal);
            }
            Some(index)
        };
        let mut parameters = Vec::with_capacity(param_count);
        for index in 0..param_count {
            let mut parameter = CallableParameterViewV1::default();
            let status = unsafe {
                __elephc_eval_callable_parameter_v1(signature.parameters, index, &mut parameter)
            };
            if status != 1 || (parameter.name.is_null() && parameter.name_length != 0) {
                return Err(EvalStatus::RuntimeFatal);
            }
            let name = if parameter.name_length == 0 {
                String::new()
            } else {
                let bytes = unsafe { std::slice::from_raw_parts(parameter.name, parameter.name_length) };
                std::str::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?.to_string()
            };
            parameters.push(NativeCallableParameterMetadata {
                name, type_tag: parameter.type_tag, by_ref: parameter.by_ref != 0,
            });
        }
        if signature.name.is_null() && signature.name_length != 0 {
            return Err(EvalStatus::RuntimeFatal);
        }
        let display_name = if signature.name_length == 0 {
            String::from("Closure")
        } else {
            let bytes = unsafe { std::slice::from_raw_parts(signature.name, signature.name_length) };
            std::str::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?.to_string()
        };
        Ok(Some(NativeCallableSignatureMetadata {
            display_name,
            parameters,
            regular_param_count,
            variadic_index,
        }))
    }

    /// Enters native code behind a complete handler and adopts the result only after status checks.
    fn invoke_native_callable(
        &mut self,
        callback: RuntimeCellHandle,
        arguments: RuntimeCellHandle,
        invocation_scope: i64,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let mut output = std::ptr::null_mut();
        let status = unsafe {
            __elephc_eval_invoke_native_callable_v2(
                callback.as_ptr(), arguments.as_ptr(), invocation_scope, &mut output,
            )
        };
        if status == 1 { return Err(EvalStatus::UnsupportedConstruct); }
        self.handle_native_cleanup_status(status)?;
        self.handle_native_call_result(output)
    }

    /// Materializes a populated synthetic `ReflectionAttribute` object for eval metadata.
    fn reflection_attribute_new(
        &mut self,
        name: &str,
        args: RuntimeCellHandle,
        target: u64,
        repeated: bool,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_reflection_attribute_new(
                name.as_ptr(),
                name.len() as u64,
                args.as_ptr(),
                target,
                if repeated { 1 } else { 0 },
            )
        })
    }

    /// Materializes a populated synthetic Reflection owner object for eval metadata.
    fn reflection_owner_new(
        &mut self,
        owner_kind: u64,
        reflected_name: &str,
        attrs: RuntimeCellHandle,
        interface_names: RuntimeCellHandle,
        trait_names: RuntimeCellHandle,
        method_names: RuntimeCellHandle,
        property_names: RuntimeCellHandle,
        method_objects: RuntimeCellHandle,
        property_objects: RuntimeCellHandle,
        parent_class: RuntimeCellHandle,
        flags: u64,
        modifiers: u64,
        method_modifiers: u64,
        constant_value: RuntimeCellHandle,
        backing_value: RuntimeCellHandle,
        constructor: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_reflection_owner_new(
                owner_kind,
                reflected_name.as_ptr(),
                reflected_name.len() as u64,
                attrs.as_ptr(),
                interface_names.as_ptr(),
                trait_names.as_ptr(),
                method_names.as_ptr(),
                property_names.as_ptr(),
                method_objects.as_ptr(),
                property_objects.as_ptr(),
                parent_class.as_ptr(),
                flags,
                modifiers,
                method_modifiers,
                constant_value.as_ptr(),
                backing_value.as_ptr(),
                constructor.as_ptr(),
            )
        })
    }

    /// Returns generated AOT ReflectionMethod flags, or `None` when no row matches.
    fn reflection_method_flags(
        &mut self,
        class_name: &str,
        method_name: &str,
    ) -> Result<Option<u64>, EvalStatus> {
        let flags = unsafe {
            __elephc_eval_reflection_method_flags(
                class_name.as_ptr(),
                class_name.len() as u64,
                method_name.as_ptr(),
                method_name.len() as u64,
            )
        };
        Ok((flags != 0).then_some(flags))
    }

    /// Returns generated AOT ReflectionMethod declaring class metadata.
    fn reflection_method_declaring_class(
        &mut self,
        class_name: &str,
        method_name: &str,
    ) -> Result<Option<String>, EvalStatus> {
        let ptr = unsafe {
            __elephc_eval_reflection_method_declaring_class(
                class_name.as_ptr(),
                class_name.len() as u64,
                method_name.as_ptr(),
                method_name.len() as u64,
            )
        };
        if ptr.is_null() {
            return Ok(None);
        }
        let handle = RuntimeCellHandle::from_raw(ptr);
        let bytes = self.string_bytes(handle)?;
        self.release(handle)?;
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| EvalStatus::RuntimeFatal)
    }

    /// Returns generated AOT ReflectionMethod names visible for one class.
    fn reflection_method_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_reflection_method_names(class_name.as_ptr(), class_name.len() as u64)
        })
    }

    /// Returns generated AOT source-file metadata for reflection source-location calls.
    fn reflection_source_file(&mut self) -> Result<Option<String>, EvalStatus> {
        let ptr = unsafe { __elephc_eval_reflection_source_file() };
        if ptr.is_null() {
            return Ok(None);
        }
        let handle = RuntimeCellHandle::from_raw(ptr);
        let bytes = self.string_bytes(handle)?;
        self.release(handle)?;
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| EvalStatus::RuntimeFatal)
    }

    /// Returns generated AOT ReflectionClass modifier flags, or `None` when no row matches.
    fn reflection_class_flags(&mut self, class_name: &str) -> Result<Option<u64>, EvalStatus> {
        let flags = unsafe {
            __elephc_eval_reflection_class_flags(class_name.as_ptr(), class_name.len() as u64)
        };
        Ok((flags != 0).then_some(flags))
    }

    /// Returns generated AOT ReflectionProperty flags, or `None` when no row matches.
    fn reflection_property_flags(
        &mut self,
        class_name: &str,
        property_name: &str,
    ) -> Result<Option<u64>, EvalStatus> {
        let flags = unsafe {
            __elephc_eval_reflection_property_flags(
                class_name.as_ptr(),
                class_name.len() as u64,
                property_name.as_ptr(),
                property_name.len() as u64,
            )
        };
        Ok((flags != 0).then_some(flags))
    }

    /// Returns generated AOT ReflectionProperty declaring class metadata.
    fn reflection_property_declaring_class(
        &mut self,
        class_name: &str,
        property_name: &str,
    ) -> Result<Option<String>, EvalStatus> {
        let ptr = unsafe {
            __elephc_eval_reflection_property_declaring_class(
                class_name.as_ptr(),
                class_name.len() as u64,
                property_name.as_ptr(),
                property_name.len() as u64,
            )
        };
        if ptr.is_null() {
            return Ok(None);
        }
        let handle = RuntimeCellHandle::from_raw(ptr);
        let bytes = self.string_bytes(handle)?;
        self.release(handle)?;
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| EvalStatus::RuntimeFatal)
    }

    /// Returns generated AOT ReflectionProperty names visible for one class.
    fn reflection_property_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_reflection_property_names(class_name.as_ptr(), class_name.len() as u64)
        })
    }

    /// Returns generated AOT ReflectionClassConstant values without visibility checks.
    fn reflection_constant_value(
        &mut self,
        class_name: &str,
        constant_name: &str,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let ptr = unsafe {
            __elephc_eval_reflection_constant_value(
                class_name.as_ptr(),
                class_name.len() as u64,
                constant_name.as_ptr(),
                constant_name.len() as u64,
            )
        };
        if ptr.is_null() {
            Ok(None)
        } else {
            Ok(Some(RuntimeCellHandle::from_raw(ptr)))
        }
    }

    /// Returns generated AOT ReflectionClassConstant flags for one constant.
    fn reflection_constant_flags(
        &mut self,
        class_name: &str,
        constant_name: &str,
    ) -> Result<Option<u64>, EvalStatus> {
        let flags = unsafe {
            __elephc_eval_reflection_constant_flags(
                class_name.as_ptr(),
                class_name.len() as u64,
                constant_name.as_ptr(),
                constant_name.len() as u64,
            )
        };
        Ok((flags != 0).then_some(flags))
    }

    /// Returns generated AOT ReflectionClassConstant declaring class metadata.
    fn reflection_constant_declaring_class(
        &mut self,
        class_name: &str,
        constant_name: &str,
    ) -> Result<Option<String>, EvalStatus> {
        let ptr = unsafe {
            __elephc_eval_reflection_constant_declaring_class(
                class_name.as_ptr(),
                class_name.len() as u64,
                constant_name.as_ptr(),
                constant_name.len() as u64,
            )
        };
        if ptr.is_null() {
            return Ok(None);
        }
        let handle = RuntimeCellHandle::from_raw(ptr);
        let bytes = self.string_bytes(handle)?;
        self.release(handle)?;
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| EvalStatus::RuntimeFatal)
    }

    /// Returns generated AOT ReflectionClassConstant names visible for one class.
    fn reflection_constant_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_reflection_constant_names(class_name.as_ptr(), class_name.len() as u64)
        })
    }

    /// Returns generated AOT interface names visible for one reflected class-like symbol.
    fn reflection_class_interface_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_reflection_class_interface_names(
                class_name.as_ptr(),
                class_name.len() as u64,
            )
        })
    }

    /// Returns generated AOT trait names visible for one reflected class-like symbol.
    fn reflection_class_trait_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_reflection_class_trait_names(
                class_name.as_ptr(),
                class_name.len() as u64,
            )
        })
    }

    /// Returns generated AOT trait alias names visible for one reflected class-like symbol.
    fn reflection_class_trait_alias_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_reflection_class_trait_alias_names(
                class_name.as_ptr(),
                class_name.len() as u64,
            )
        })
    }

    /// Returns generated AOT trait alias sources visible for one reflected class-like symbol.
    fn reflection_class_trait_alias_sources(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe {
            __elephc_eval_reflection_class_trait_alias_sources(
                class_name.as_ptr(),
                class_name.len() as u64,
            )
        })
    }

    };
}

pub(super) use impl_reflection_ops;
