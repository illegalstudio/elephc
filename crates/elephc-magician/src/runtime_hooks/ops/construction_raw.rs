//! Purpose:
//! Defines object construction, class-like queries, raw value conversion, and
//! raw heap/string ownership methods for the runtime adapter.
//!
//! Called from:
//! - The single `RuntimeValueOps for ElephcRuntimeOps` implementation in `super`.
//!
//! Key details:
//! - Raw retain/release pairs preserve the generated runtime ownership contract.

macro_rules! impl_construction_raw_ops {
    () => {

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
                self.context.cast(),
            )
        };
        unsafe {
            __elephc_eval_value_release(arg_array.as_ptr());
        }
        if ok == 0 {
            self.take_pending_native_throwable()
                .map_or(Err(EvalStatus::RuntimeFatal), |thrown| {
                    self.schedule_pending_throw(thrown)?;
                    Err(EvalStatus::UncaughtThrowable)
                })
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

    /// Creates an invoker-only by-reference marker for a staged Mixed slot.
    fn invoker_ref_cell(
        &mut self,
        slot: *mut RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_invoker_ref_cell(slot) })
    }

    /// Creates an invoker-only by-reference marker for a staged raw one-word slot.
    fn invoker_raw_ref_cell(
        &mut self,
        slot: *mut std::ffi::c_void,
        source_tag: u64,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_invoker_raw_ref_cell(slot, source_tag) })
    }

    /// Extracts the low raw payload word from a boxed Mixed cell.
    fn raw_value_word(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        Ok(unsafe { __elephc_eval_value_raw_word(value.as_ptr()) })
    }

    /// Extracts the high raw payload word from a boxed Mixed cell.
    fn raw_value_high_word(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        Ok(unsafe { __elephc_eval_value_raw_high_word(value.as_ptr()) })
    }

    /// Duplicates one raw string payload for owned native by-reference staging.
    fn retain_raw_string_words(&mut self, ptr: u64, len: u64) -> Result<(u64, u64), EvalStatus> {
        let mut out_len = 0;
        let out_ptr = unsafe { __elephc_eval_value_retain_raw_string(ptr, len, &mut out_len) };
        Ok((out_ptr, out_len))
    }

    /// Boxes one raw string payload as a Mixed string cell.
    fn raw_string_value(&mut self, ptr: u64, len: u64) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_from_raw_string(ptr, len) })
    }

    /// Releases one raw string payload owned by native by-reference staging.
    fn release_raw_string_words(&mut self, ptr: u64, len: u64) -> Result<(), EvalStatus> {
        unsafe {
            __elephc_eval_value_release_raw_string(ptr, len);
        }
        Ok(())
    }

    /// Retains one raw heap payload word for owned native by-reference staging.
    fn retain_raw_heap_word(&mut self, word: u64) -> Result<u64, EvalStatus> {
        Ok(unsafe { __elephc_eval_value_retain_raw_heap_word(word) })
    }

    /// Boxes one one-word raw payload as a Mixed cell with the provided runtime tag.
    fn raw_word_value(
        &mut self,
        source_tag: u64,
        word: u64,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_from_raw_word(source_tag, word) })
    }

    /// Boxes one raw heap payload word as a Mixed cell using its runtime heap kind.
    fn raw_heap_word_value(&mut self, word: u64) -> Result<RuntimeCellHandle, EvalStatus> {
        Self::handle(unsafe { __elephc_eval_value_from_raw_heap_word(word) })
    }

    /// Releases one raw heap payload word owned by native by-reference staging.
    fn release_raw_heap_word(&mut self, word: u64) -> Result<(), EvalStatus> {
        unsafe {
            __elephc_eval_value_release_raw_heap_word(word);
        }
        Ok(())
    }

    };
}

pub(super) use impl_construction_raw_ops;
