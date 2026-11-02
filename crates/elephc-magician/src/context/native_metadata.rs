//! Purpose:
//! Stores generated class hierarchy, attributes, properties, and abstract contract metadata.
//!
//! Called from:
//! - FFI registration, declaration validation, Reflection, and property access.
//!
//! Key details:
//! - Native member keys are normalized consistently with eval class-like lookups.

use super::*;

/// One array-valued compiled class constant, as registered by the generated program.
#[derive(Clone, Debug)]
pub struct NativeClassConstantDefault {
    /// The class (or interface) that declares the constant, for visibility checks.
    pub declaring_class: String,
    /// The constant's declared visibility.
    pub visibility: EvalVisibility,
    /// The constant's value, materialized afresh on every read.
    pub default: NativeCallableDefault,
}

impl ElephcEvalContext {
    /// Records the first class-initialization Error separately from individual reflected defaults.
    pub fn define_native_class_default_error(&mut self, class_name: &str, message: String) -> bool {
        let key = normalize_class_name(class_name);
        if key.is_empty() || message.is_empty() { return false; }
        self.native_class_default_errors.insert(key, message).is_none()
    }

    /// Returns a native class's initialization failure without evaluating a native callback.
    pub fn native_class_default_error(&self, class_name: &str) -> Option<&str> {
        self.native_class_default_errors.get(&normalize_class_name(class_name)).map(String::as_str)
    }


    /// Registers an array-valued compiled class constant under one class that can read it.
    ///
    /// The generated `__elephc_eval_value_class_constant_get` helper boxes scalar and enum-case
    /// constants only, so `ReflectionCaster::UNSET_CLOSURE_FILE_INFO` -- an array built with
    /// `__CLASS__` -- read from interpreted code was "Undefined constant". The generated program
    /// registers each such constant here, under every class that inherits it.
    pub fn define_native_class_constant_default(
        &mut self,
        class_name: &str,
        constant_name: &str,
        constant: NativeClassConstantDefault,
    ) -> bool {
        let key = native_constant_key(class_name, constant_name);
        if key.0.is_empty() || key.1.is_empty() {
            return false;
        }
        self.native_class_constant_defaults
            .insert(key, constant)
            .is_none()
    }

    /// Returns an array-valued compiled class constant registered for one class.
    pub fn native_class_constant_default(
        &self,
        class_name: &str,
        constant_name: &str,
    ) -> Option<NativeClassConstantDefault> {
        self.native_class_constant_defaults
            .get(&native_constant_key(class_name, constant_name))
            .cloned()
    }

    /// Registers generated AOT parent metadata without replacing an existing hierarchy edge.
    pub fn define_native_class_parent(&mut self, class_name: &str, parent_name: &str) -> bool {
        let class_key = normalize_class_name(class_name);
        let parent_name = parent_name.trim_start_matches('\\');
        if class_key.is_empty() || parent_name.is_empty() {
            return false;
        }
        if let Some(existing_parent) = self.native_class_parents.get(&class_key) {
            return existing_parent.eq_ignore_ascii_case(parent_name);
        }
        self.native_class_parents
            .insert(class_key, parent_name.to_string());
        true
    }

    /// Returns generated AOT parent metadata by PHP class name.
    pub fn native_class_parent(&self, class_name: &str) -> Option<&str> {
        self.native_class_parents
            .get(&normalize_class_name(class_name))
            .map(String::as_str)
    }

    /// Appends generated AOT class attribute metadata for eval reflection.
    pub fn define_native_class_attribute(
        &mut self,
        class_name: &str,
        attribute: EvalAttribute,
    ) -> bool {
        let key = normalize_class_name(class_name);
        if key.is_empty() {
            return false;
        }
        self.native_class_attributes
            .entry(key)
            .or_default()
            .push(attribute);
        true
    }

    /// Returns generated AOT class attribute metadata by PHP class name.
    pub fn native_class_attributes(&self, class_name: &str) -> Vec<EvalAttribute> {
        self.native_class_attributes
            .get(&normalize_class_name(class_name))
            .cloned()
            .unwrap_or_default()
    }

    /// Appends generated AOT method attribute metadata for eval reflection.
    pub fn define_native_method_attribute(
        &mut self,
        class_name: &str,
        method_name: &str,
        attribute: EvalAttribute,
    ) -> bool {
        let key = native_method_key(class_name, method_name);
        if key.0.is_empty() || key.1.is_empty() {
            return false;
        }
        self.native_method_attributes
            .entry(key)
            .or_default()
            .push(attribute);
        true
    }

    /// Returns generated AOT method attribute metadata by PHP class and method name.
    pub fn native_method_attributes(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Vec<EvalAttribute> {
        self.native_method_attributes
            .get(&native_method_key(class_name, method_name))
            .cloned()
            .unwrap_or_default()
    }

    /// Appends generated AOT class-constant attribute metadata for eval reflection.
    pub fn define_native_constant_attribute(
        &mut self,
        class_name: &str,
        constant_name: &str,
        attribute: EvalAttribute,
    ) -> bool {
        let key = native_constant_key(class_name, constant_name);
        if key.0.is_empty() || key.1.is_empty() {
            return false;
        }
        self.native_constant_attributes
            .entry(key)
            .or_default()
            .push(attribute);
        true
    }

    /// Returns generated AOT class-constant attribute metadata by PHP class and constant name.
    pub fn native_constant_attributes(
        &self,
        class_name: &str,
        constant_name: &str,
    ) -> Vec<EvalAttribute> {
        self.native_constant_attributes
            .get(&native_constant_key(class_name, constant_name))
            .cloned()
            .unwrap_or_default()
    }

    /// Defines generated AOT interface property-hook metadata for eval validation.
    pub fn define_native_interface_property_requirement(
        &mut self,
        interface_name: &str,
        declaring_interface_name: &str,
        property: EvalInterfaceProperty,
    ) -> bool {
        let key = normalize_class_name(interface_name);
        let owner = declaring_interface_name.trim_start_matches('\\').to_string();
        if key.is_empty() || owner.is_empty() || property.name().is_empty() {
            return false;
        }
        let requirements = self.native_interface_properties.entry(key).or_default();
        if requirements.iter().any(|(_, existing)| existing.name() == property.name()) {
            return false;
        }
        requirements.push((owner, property));
        true
    }

    /// Returns generated AOT interface property-hook metadata by interface name.
    pub fn native_interface_property_requirements(
        &self,
        interface_name: &str,
    ) -> Vec<(String, EvalInterfaceProperty)> {
        self.native_interface_properties
            .get(&normalize_class_name(interface_name))
            .cloned()
            .unwrap_or_default()
    }

    /// Defines generated AOT abstract class property-hook metadata for eval validation.
    pub fn define_native_abstract_property_requirement(
        &mut self,
        class_name: &str,
        declaring_class_name: &str,
        property: EvalInterfaceProperty,
    ) -> bool {
        let key = normalize_class_name(class_name);
        let owner = declaring_class_name.trim_start_matches('\\').to_string();
        if key.is_empty() || owner.is_empty() || property.name().is_empty() {
            return false;
        }
        let requirements = self.native_abstract_properties.entry(key).or_default();
        if requirements
            .iter()
            .any(|(_, existing)| existing.name() == property.name())
        {
            return false;
        }
        requirements.push((owner, property));
        true
    }

    /// Returns generated AOT abstract class property-hook metadata by class name.
    pub fn native_abstract_property_requirements(
        &self,
        class_name: &str,
    ) -> Vec<(String, EvalInterfaceProperty)> {
        self.native_abstract_properties
            .get(&normalize_class_name(class_name))
            .cloned()
            .unwrap_or_default()
    }

    /// Defines generated AOT property type metadata for eval reflection.
    pub fn define_native_property_type(
        &mut self,
        class_name: &str,
        property_name: &str,
        property_type: EvalParameterType,
    ) -> bool {
        let key = native_property_key(class_name, property_name);
        if key.0.is_empty() || key.1.is_empty() {
            return false;
        }
        self.native_property_types
            .insert(key, property_type)
            .is_none()
    }

    /// Returns generated AOT property type metadata by PHP class and property name.
    pub fn native_property_type(
        &self,
        class_name: &str,
        property_name: &str,
    ) -> Option<EvalParameterType> {
        self.native_property_types
            .get(&native_property_key(class_name, property_name))
            .cloned()
    }

    /// Defines generated AOT property default metadata for eval reflection.
    pub fn define_native_property_default(
        &mut self,
        class_name: &str,
        property_name: &str,
        default: NativeCallableDefault,
    ) -> bool {
        let key = native_property_key(class_name, property_name);
        if key.0.is_empty() || key.1.is_empty() {
            return false;
        }
        self.native_property_defaults.insert(key, default).is_none()
    }

    /// Returns generated AOT property default metadata by PHP class and property name.
    pub fn native_property_default(
        &self,
        class_name: &str,
        property_name: &str,
    ) -> Option<NativeCallableDefault> {
        self.native_property_defaults
            .get(&native_property_key(class_name, property_name))
            .cloned()
    }

    /// Defines a deferred native property default without materializing its value.
    pub fn define_native_property_default_error(
        &mut self, class_name: &str, property_name: &str, message: String,
    ) -> bool {
        let key = native_property_key(class_name, property_name);
        if key.0.is_empty() || key.1.is_empty() || message.is_empty() { return false; }
        self.native_property_default_errors.insert(key, message).is_none()
    }

    /// Returns a native default's per-slot error at its eventual reflection boundary.
    pub fn native_property_default_error(&self, class_name: &str, property_name: &str) -> Option<&str> {
        self.native_property_default_errors.get(&native_property_key(class_name, property_name))
            .map(String::as_str)
    }

    /// Appends generated AOT property attribute metadata for eval reflection.
    pub fn define_native_property_attribute(
        &mut self,
        class_name: &str,
        property_name: &str,
        attribute: EvalAttribute,
    ) -> bool {
        let key = native_property_key(class_name, property_name);
        if key.0.is_empty() || key.1.is_empty() {
            return false;
        }
        self.native_property_attributes
            .entry(key)
            .or_default()
            .push(attribute);
        true
    }

    /// Returns generated AOT property attribute metadata by PHP class and property name.
    pub fn native_property_attributes(
        &self,
        class_name: &str,
        property_name: &str,
    ) -> Vec<EvalAttribute> {
        self.native_property_attributes
            .get(&native_property_key(class_name, property_name))
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies a registration made after adopting the published table forks a private copy.
    ///
    /// Contexts no longer deep-copy the AOT registration stream: they adopt the first
    /// context's tables by reference count. That is only sound while a later write cannot
    /// reach the shared allocation, so exercise the one registration setter that can run
    /// after a sync and assert the snapshot every other context still holds is unchanged.
    #[test]
    fn a_registration_after_adopting_the_shared_table_forks_it() {
        let mut publisher = ElephcEvalContext::new();
        assert!(publisher.define_native_property_default(
            "Publisher",
            "kept",
            NativeCallableDefault::Int(7),
        ));

        // What `publish_global_eval_aot_metadata` stores and `sync_global_eval_aot_metadata`
        // hands out: one allocation, reference-counted rather than copied.
        let published = publisher.native_property_defaults.share();
        let mut adopter = ElephcEvalContext::new();
        adopter.native_property_defaults = SharedNative::from_shared(Arc::clone(&published));
        assert_eq!(published.len(), 1);

        assert!(adopter.define_native_property_default(
            "Adopter",
            "added",
            NativeCallableDefault::Bool(true),
        ));

        assert_eq!(
            published.len(),
            1,
            "a write made after the sync must not reach the published snapshot",
        );
        assert!(published
            .get(&native_property_key("Adopter", "added"))
            .is_none());

        // The adopting context keeps both the inherited entry and its own.
        assert_eq!(
            adopter.native_property_default("Publisher", "kept"),
            Some(NativeCallableDefault::Int(7)),
        );
        assert_eq!(
            adopter.native_property_default("Adopter", "added"),
            Some(NativeCallableDefault::Bool(true)),
        );

        // And the publishing context never sees another context's later registration.
        assert_eq!(publisher.native_property_default("Adopter", "added"), None);
    }
}
