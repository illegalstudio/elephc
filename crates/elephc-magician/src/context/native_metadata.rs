//! Purpose:
//! Stores generated class hierarchy, attributes, properties, and abstract contract metadata.
//!
//! Called from:
//! - FFI registration, declaration validation, Reflection, and property access.
//!
//! Key details:
//! - Native member keys are normalized consistently with eval class-like lookups.

use super::*;

impl ElephcEvalContext {
    /// Defines generated AOT parent metadata for eval `parent::` resolution.
    pub fn define_native_class_parent(&mut self, class_name: &str, parent_name: &str) -> bool {
        let class_key = normalize_class_name(class_name);
        let parent_name = parent_name.trim_start_matches('\\');
        if class_key.is_empty() || parent_name.is_empty() {
            return false;
        }
        self.native_class_parents
            .insert(class_key, parent_name.to_string())
            .is_none()
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
