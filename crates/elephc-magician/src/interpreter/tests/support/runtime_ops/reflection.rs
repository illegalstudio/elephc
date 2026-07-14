//! Purpose:
//! Defines fake Reflection object and metadata-query trait methods for
//! interpreter tests.
//!
//! Called from:
//! - The single `RuntimeValueOps for FakeOps` implementation in `super`.
//!
//! Key details:
//! - All calls delegate to the fake reflection object model.

macro_rules! impl_fake_reflection_ops {
    () => {

    /// Materializes one fake `ReflectionAttribute` object for eval metadata tests.
    fn reflection_attribute_new(
        &mut self,
        name: &str,
        args: RuntimeCellHandle,
        target: u64,
        repeated: bool,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_reflection_attribute_new(name, args, target, repeated)
    }
    /// Materializes one fake Reflection owner object for eval metadata tests.
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
        self.runtime_reflection_owner_new(
            owner_kind,
            reflected_name,
            attrs,
            interface_names,
            trait_names,
            method_names,
            property_names,
            method_objects,
            property_objects,
            parent_class,
            flags,
            modifiers,
            method_modifiers,
            constant_value,
            backing_value,
            constructor,
        )
    }
    /// Reports fake generated AOT ReflectionMethod flags for metadata bridge tests.
    fn reflection_method_flags(
        &mut self,
        class_name: &str,
        method_name: &str,
    ) -> Result<Option<u64>, EvalStatus> {
        self.runtime_reflection_method_flags(class_name, method_name)
    }
    /// Reports fake generated AOT ReflectionMethod declaring classes for metadata bridge tests.
    fn reflection_method_declaring_class(
        &mut self,
        class_name: &str,
        method_name: &str,
    ) -> Result<Option<String>, EvalStatus> {
        self.runtime_reflection_method_declaring_class(class_name, method_name)
    }
    /// Reports fake generated AOT ReflectionMethod names for metadata bridge tests.
    fn reflection_method_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_reflection_method_names(class_name)
    }
    /// Reports fake generated AOT ReflectionClass flags for metadata bridge tests.
    fn reflection_class_flags(&mut self, class_name: &str) -> Result<Option<u64>, EvalStatus> {
        self.runtime_reflection_class_flags(class_name)
    }
    /// Reports fake generated AOT ReflectionProperty flags for metadata bridge tests.
    fn reflection_property_flags(
        &mut self,
        class_name: &str,
        property_name: &str,
    ) -> Result<Option<u64>, EvalStatus> {
        self.runtime_reflection_property_flags(class_name, property_name)
    }
    /// Reports fake generated AOT ReflectionProperty declaring classes for metadata bridge tests.
    fn reflection_property_declaring_class(
        &mut self,
        class_name: &str,
        property_name: &str,
    ) -> Result<Option<String>, EvalStatus> {
        self.runtime_reflection_property_declaring_class(class_name, property_name)
    }
    /// Reports fake generated AOT ReflectionProperty names for metadata bridge tests.
    fn reflection_property_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_reflection_property_names(class_name)
    }
    /// Reports no fake generated AOT ReflectionClassConstant value.
    fn reflection_constant_value(
        &mut self,
        _class_name: &str,
        _constant_name: &str,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        Ok(None)
    }
    /// Reports no fake generated AOT ReflectionClassConstant flags.
    fn reflection_constant_flags(
        &mut self,
        _class_name: &str,
        _constant_name: &str,
    ) -> Result<Option<u64>, EvalStatus> {
        Ok(None)
    }
    /// Reports no fake generated AOT ReflectionClassConstant declaring class.
    fn reflection_constant_declaring_class(
        &mut self,
        _class_name: &str,
        _constant_name: &str,
    ) -> Result<Option<String>, EvalStatus> {
        Ok(None)
    }
    /// Reports an empty fake generated AOT ReflectionClassConstant name list.
    fn reflection_constant_names(
        &mut self,
        _class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_string_array_new(0)
    }
    /// Reports fake generated/AOT ReflectionClass interface names for metadata bridge tests.
    fn reflection_class_interface_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_reflection_class_interface_names(class_name)
    }
    /// Reports fake generated/AOT ReflectionClass trait names for metadata bridge tests.
    fn reflection_class_trait_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_reflection_class_trait_names(class_name)
    }
    /// Reports fake generated/AOT ReflectionClass trait alias names for metadata bridge tests.
    fn reflection_class_trait_alias_names(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_reflection_class_trait_alias_names(class_name)
    }
    /// Reports fake generated/AOT ReflectionClass trait alias sources for metadata bridge tests.
    fn reflection_class_trait_alias_sources(
        &mut self,
        class_name: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        self.runtime_reflection_class_trait_alias_sources(class_name)
    }

    };
}

pub(super) use impl_fake_reflection_ops;
