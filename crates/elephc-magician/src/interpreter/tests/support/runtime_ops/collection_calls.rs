//! Purpose:
//! Defines fake array, property, object-call, and static-call trait methods for
//! interpreter tests.
//!
//! Called from:
//! - The single `RuntimeValueOps for FakeOps` implementation in `super`.
//!
//! Key details:
//! - Methods delegate to focused fake runtime helpers without altering handles.

macro_rules! impl_fake_collection_call_ops {
    () => {
    /// Creates a fake indexed array cell.
    fn array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_array_new(capacity)
    }
    /// Creates a fake direct-string indexed array cell.
    fn string_array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_string_array_new(capacity)
    }
    /// Appends one string to a fake direct-string indexed array cell.
    fn string_array_push(
        &mut self,
        array: RuntimeCellHandle,
        value: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_string_array_push(array, value)
    }
    /// Creates a fake associative array cell.
    fn assoc_new(&mut self, _capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_assoc_new(_capacity)
    }
    /// Reads one fake indexed array element.
    fn array_get(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_array_get(array, index)
    }
    /// Checks whether a fake array has the requested key without reading its value.
    fn array_key_exists(
        &mut self,
        key: RuntimeCellHandle,
        array: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_array_key_exists(key, array)
    }
    /// Returns one fake foreach key by insertion-order position.
    fn array_iter_key(
        &mut self,
        array: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_array_iter_key(array, position)
    }
    /// Writes one fake indexed or associative array element.
    fn array_set(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_array_set(array, index, value)
    }
    /// Reads one fake object property by name.
    fn property_get(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_property_get(object, property)
    }
    /// Checks whether one fake object property exists by name.
    fn property_is_initialized(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
    ) -> Result<bool, EvalStatus> {
        self.runtime_property_is_initialized(object, property)
    }
    /// Writes one fake object property by name.
    fn property_set(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
        value: RuntimeCellHandle,
    ) -> Result<(), EvalStatus> {
        self.runtime_property_set(object, property, value)
    }
    /// Reports no fake AOT static property match.
    fn static_property_get(
        &mut self,
        _class_name: &str,
        _property: &str,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        Ok(None)
    }
    /// Reports no fake AOT static property initialization match.
    fn static_property_is_initialized(
        &mut self,
        _class_name: &str,
        _property: &str,
    ) -> Result<bool, EvalStatus> {
        Ok(false)
    }
    /// Reports a failed fake AOT static property write.
    fn static_property_set(
        &mut self,
        _class_name: &str,
        _property: &str,
        _value: RuntimeCellHandle,
    ) -> Result<bool, EvalStatus> {
        Ok(false)
    }
    /// Reports no fake AOT class constant match.
    fn class_constant_get(
        &mut self,
        _class_name: &str,
        _constant: &str,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        Ok(None)
    }
    /// Creates one shallow fake object clone.
    fn object_clone_shallow(
        &mut self,
        object: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_object_clone_shallow(object)
    }
    /// Returns the number of fake object properties in insertion order.
    fn object_property_len(&mut self, object: RuntimeCellHandle) -> Result<usize, EvalStatus> {
        self.runtime_object_property_len(object)
    }
    /// Returns one fake object property key by insertion-order position.
    fn object_property_iter_key(
        &mut self,
        object: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_object_property_iter_key(object, position)
    }
    /// Calls one fake object method by name.
    fn method_call(
        &mut self,
        object: RuntimeCellHandle,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_method_call(object, method, args)
    }
    /// Calls one fake static runtime method by class and method name.
    fn static_method_call(
        &mut self,
        class_name: &str,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_static_method_call(class_name, method, args)
    }

    };
}

pub(super) use impl_fake_collection_call_ops;
