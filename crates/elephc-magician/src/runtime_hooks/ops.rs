//! Purpose:
//! Implements RuntimeValueOps by delegating each eval value operation to the
//! generated elephc runtime wrapper symbols.
//!
//! Called from:
//! - `crate::interpreter` when executing EvalIR in non-test builds.
//!
//! Key details:
//! - Every returned runtime pointer is checked before becoming a handle.
//! - Temporary argument arrays are released after object and method bridge calls.

use super::externs::*;
use super::tags::{bitwise_op_tag, compare_op_tag};
use super::ElephcRuntimeOps;
use crate::errors::EvalStatus;
use crate::eval_ir::EvalBinOp;
use crate::interpreter::RuntimeValueOps;
use crate::value::RuntimeCellHandle;

#[cfg(not(test))]
impl RuntimeValueOps for ElephcRuntimeOps {
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
        let result = Self::handle(unsafe {
            __elephc_eval_value_method_call(
                object.as_ptr(),
                method.as_ptr(),
                method.len() as u64,
                arg_array.as_ptr(),
                scope_ptr,
                scope_len,
            )
        });
        unsafe {
            __elephc_eval_value_release(arg_array.as_ptr());
        }
        result
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
        let result = Self::handle(unsafe {
            __elephc_eval_value_static_method_call(
                class_name.as_ptr(),
                class_name.len() as u64,
                method.as_ptr(),
                method.len() as u64,
                arg_array.as_ptr(),
                scope_ptr,
                scope_len,
            )
        });
        unsafe {
            __elephc_eval_value_release(arg_array.as_ptr());
        }
        result
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

    /// Calls an AOT constructor through the generated user bridge when one exists.
    fn construct_object(
        &mut self,
        object: RuntimeCellHandle,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<(), EvalStatus> {
        let (scope_ptr, scope_len) = self.current_class_scope_abi();
        let arg_array = Self::arg_array(args)?;
        let ok = unsafe {
            __elephc_eval_value_construct_object(
                object.as_ptr(),
                arg_array.as_ptr(),
                scope_ptr,
                scope_len,
            )
        };
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

    /// Returns whether the generated AOT interface-name table contains the requested interface.
    fn interface_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_interface_exists(name.as_ptr(), name.len() as u64) != 0 })
    }

    /// Returns whether the generated AOT trait-name table contains the requested trait.
    fn trait_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_trait_exists(name.as_ptr(), name.len() as u64) != 0 })
    }

    /// Returns whether the generated AOT enum-name table contains the requested enum.
    fn enum_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        Ok(unsafe { __elephc_eval_enum_exists(name.as_ptr(), name.len() as u64) != 0 })
    }

    /// Tests a boxed Mixed object against generated class/interface metadata.
    fn object_is_a(
        &mut self,
        object_or_class: RuntimeCellHandle,
        target_class: &str,
        exclude_self: bool,
    ) -> Result<bool, EvalStatus> {
        Ok(unsafe {
            __elephc_eval_value_is_a(
                object_or_class.as_ptr(),
                target_class.as_ptr(),
                target_class.len() as u64,
                u64::from(exclude_self),
            ) != 0
        })
    }

    /// Returns a boxed Mixed string naming a boxed Mixed object's runtime class.
    fn object_class_name(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_object_class_name(object.as_ptr()) })
    }

    /// Returns a boxed Mixed string naming a boxed Mixed object's or class string's parent class.
    fn parent_class_name(
        &mut self,
        object_or_class: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_parent_class_name(object_or_class.as_ptr()) })
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
