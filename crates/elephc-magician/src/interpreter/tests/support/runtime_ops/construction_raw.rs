//! Purpose:
//! Defines fake construction, class queries, raw words, and raw string/heap
//! ownership trait methods.
//!
//! Called from:
//! - The single `RuntimeValueOps for FakeOps` implementation in `super`.
//!
//! Key details:
//! - Opaque fake handles preserve the staging and release behavior under test.

macro_rules! impl_fake_construction_raw_ops {
    () => {

    /// Creates one fake object for eval `new` unit tests.
    fn new_object(&mut self, _class_name: &str) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_new_object(_class_name)
    }
    /// Applies fake constructor side effects for eval `new` unit tests.
    fn construct_object(
        &mut self,
        object: RuntimeCellHandle,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<(), EvalStatus> {
        self.runtime_construct_object(object, args)
    }
    /// Reports one fake AOT class for eval `class_exists` unit tests.
    fn class_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        self.runtime_class_exists(name)
    }
    /// Reports one fake AOT interface for eval `interface_exists` unit tests.
    fn interface_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        self.runtime_interface_exists(name)
    }
    /// Reports one fake AOT trait for eval `trait_exists` unit tests.
    fn trait_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        self.runtime_trait_exists(name)
    }
    /// Reports one fake AOT enum for eval `enum_exists` unit tests.
    fn enum_exists(&mut self, name: &str) -> Result<bool, EvalStatus> {
        self.runtime_enum_exists(name)
    }
    /// Reports fake class relations for eval `is_a` and `is_subclass_of` unit tests.
    fn object_is_a(
        &mut self,
        object_or_class: RuntimeCellHandle,
        target_class: &str,
        exclude_self: bool,
    ) -> Result<bool, EvalStatus> {
        self.runtime_object_is_a(object_or_class, target_class, exclude_self)
    }
    /// Returns a fake PHP class name for object-tagged test values.
    fn object_class_name(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_object_class_name(object)
    }
    /// Returns fake parent-class names for eval introspection unit tests.
    fn parent_class_name(
        &mut self,
        object_or_class: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_parent_class_name(object_or_class)
    }
    /// Returns the visible element count for fake array values.
    fn array_len(&mut self, array: RuntimeCellHandle) -> Result<usize, EvalStatus> {
        self.runtime_array_len(array)
    }
    /// Returns whether a fake runtime cell is an indexed or associative array.
    fn is_array_like(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        self.runtime_is_array_like(value)
    }
    /// Returns whether a fake runtime cell is null.
    fn is_null(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        self.runtime_is_null(value)
    }
    /// Returns the fake runtime tag corresponding to a test value.
    fn type_tag(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        self.runtime_type_tag(value)
    }
    /// Creates a fake invoker-only by-reference marker.
    fn invoker_ref_cell(
        &mut self,
        slot: *mut RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::InvokerRefCell(slot as usize)))
    }
    /// Creates a fake invoker-only raw by-reference marker.
    fn invoker_raw_ref_cell(
        &mut self,
        slot: *mut std::ffi::c_void,
        _source_tag: u64,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(self.alloc(FakeValue::InvokerRefCell(slot as usize)))
    }
    /// Extracts one fake low payload word for raw by-reference staging.
    fn raw_value_word(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        Ok(match self.get(value) {
            FakeValue::Bool(value) => u64::from(value),
            FakeValue::Float(value) => value.to_bits(),
            FakeValue::Int(value) => value as u64,
            FakeValue::String(_) | FakeValue::Bytes(_) => value.as_ptr() as u64,
            FakeValue::Array(_)
            | FakeValue::Assoc(_)
            | FakeValue::Object(_)
            | FakeValue::Iterator { .. } => value.as_ptr() as u64,
            _ => 0,
        })
    }
    /// Extracts one fake high payload word for raw by-reference staging.
    fn raw_value_high_word(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus> {
        Ok(match self.get(value) {
            FakeValue::String(value) => value.len() as u64,
            FakeValue::Bytes(value) => value.len() as u64,
            _ => 0,
        })
    }
    /// Retains a fake raw string payload for native by-reference staging.
    fn retain_raw_string_words(&mut self, ptr: u64, len: u64) -> Result<(u64, u64), EvalStatus> {
        self.runtime_retain(RuntimeCellHandle::from_raw(ptr as *mut RuntimeCell))?;
        Ok((ptr, len))
    }
    /// Converts a fake raw string payload back to its stable fake handle.
    fn raw_string_value(&mut self, ptr: u64, _len: u64) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(RuntimeCellHandle::from_raw(ptr as *mut RuntimeCell))
    }
    /// Records release of a fake raw string payload owned by a staged slot.
    fn release_raw_string_words(&mut self, ptr: u64, _len: u64) -> Result<(), EvalStatus> {
        self.runtime_release(RuntimeCellHandle::from_raw(ptr as *mut RuntimeCell))
    }
    /// Retains a fake raw heap word for native by-reference staging.
    fn retain_raw_heap_word(&mut self, word: u64) -> Result<u64, EvalStatus> {
        self.runtime_retain(RuntimeCellHandle::from_raw(word as *mut RuntimeCell))?;
        Ok(word)
    }
    /// Boxes one fake one-word raw payload with the provided runtime tag.
    fn raw_word_value(
        &mut self,
        source_tag: u64,
        word: u64,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        match source_tag {
            EVAL_TAG_INT => self.runtime_int(word as i64),
            EVAL_TAG_FLOAT => self.runtime_float(f64::from_bits(word)),
            EVAL_TAG_BOOL => self.runtime_bool_value(word != 0),
            EVAL_TAG_RESOURCE => self.runtime_resource(word as i64),
            EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE => {
                Ok(RuntimeCellHandle::from_raw(word as *mut RuntimeCell))
            }
            _ => Err(EvalStatus::RuntimeFatal),
        }
    }
    /// Converts a fake raw heap word back to its stable fake handle.
    fn raw_heap_word_value(&mut self, word: u64) -> Result<RuntimeCellHandle, EvalStatus> {
        Ok(RuntimeCellHandle::from_raw(word as *mut RuntimeCell))
    }
    /// Records release of a fake raw heap word owned by a staged slot.
    fn release_raw_heap_word(&mut self, word: u64) -> Result<(), EvalStatus> {
        self.runtime_release(RuntimeCellHandle::from_raw(word as *mut RuntimeCell))
    }

    };
}

pub(super) use impl_fake_construction_raw_ops;
