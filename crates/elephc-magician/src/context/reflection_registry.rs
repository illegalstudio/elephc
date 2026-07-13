//! Purpose:
//! Registers synthetic Reflection owner identities and their eval metadata targets.
//!
//! Called from:
//! - Reflection object construction and reflected method dispatch.
//!
//! Key details:
//! - Class, function, method, property, constant, and closure targets use stable runtime identities.

use super::*;

impl ElephcEvalContext {
    /// Records eval-declared attribute metadata for one synthetic ReflectionAttribute object.
    pub fn register_eval_reflection_attribute(
        &mut self,
        identity: u64,
        attribute: EvalAttribute,
        target: u64,
        repeated: bool,
    ) {
        self.eval_reflection_attributes.insert(
            identity,
            EvalReflectionAttributeMetadata::new(attribute, target, repeated),
        );
    }

    /// Returns eval-declared attribute metadata attached to a synthetic ReflectionAttribute.
    pub fn eval_reflection_attribute(
        &self,
        identity: u64,
    ) -> Option<&EvalReflectionAttributeMetadata> {
        self.eval_reflection_attributes.get(&identity)
    }

    /// Records reflected class metadata for one synthetic ReflectionClass object.
    pub fn register_eval_reflection_class(&mut self, identity: u64, class_name: &str) {
        self.eval_reflection_classes
            .insert(identity, class_name.trim_start_matches('\\').to_string());
    }

    /// Returns the reflected class name attached to a synthetic ReflectionClass.
    pub fn eval_reflection_class_name(&self, identity: u64) -> Option<&str> {
        self.eval_reflection_classes
            .get(&identity)
            .map(String::as_str)
    }

    /// Records reflected function metadata for one synthetic ReflectionFunction object.
    pub fn register_eval_reflection_function(&mut self, identity: u64, function_name: &str) {
        self.eval_reflection_functions
            .insert(identity, function_name.trim_start_matches('\\').to_string());
    }

    /// Returns the reflected function name attached to a synthetic ReflectionFunction.
    pub fn eval_reflection_function_name(&self, identity: u64) -> Option<&str> {
        self.eval_reflection_functions
            .get(&identity)
            .map(String::as_str)
    }

    /// Records the callable target behind a `Closure` reflected as a function.
    pub fn register_eval_reflection_function_closure_target(
        &mut self,
        identity: u64,
        target: EvalClosureObjectTarget,
    ) {
        self.eval_reflection_function_closure_targets
            .insert(identity, target);
    }

    /// Returns the callable target retained for a reflected `Closure` object.
    pub fn eval_reflection_function_closure_target(
        &self,
        identity: u64,
    ) -> Option<&EvalClosureObjectTarget> {
        self.eval_reflection_function_closure_targets
            .get(&identity)
    }

    /// Records reflected method metadata for one synthetic ReflectionMethod object.
    pub fn register_eval_reflection_method(
        &mut self,
        identity: u64,
        declaring_class: &str,
        method_name: &str,
    ) {
        self.eval_reflection_methods.insert(
            identity,
            (
                declaring_class.trim_start_matches('\\').to_string(),
                method_name.to_string(),
            ),
        );
    }

    /// Returns the declaring class and method name attached to a synthetic ReflectionMethod.
    pub fn eval_reflection_method(&self, identity: u64) -> Option<(&str, &str)> {
        self.eval_reflection_methods
            .get(&identity)
            .map(|(class, method)| (class.as_str(), method.as_str()))
    }

    /// Records reflected property metadata for one synthetic ReflectionProperty object.
    pub fn register_eval_reflection_property(
        &mut self,
        identity: u64,
        declaring_class: &str,
        property_name: &str,
    ) {
        self.eval_reflection_properties.insert(
            identity,
            (
                declaring_class.trim_start_matches('\\').to_string(),
                property_name.to_string(),
            ),
        );
        self.eval_dynamic_reflection_properties.remove(&identity);
    }

    /// Records reflected dynamic-property metadata for one synthetic ReflectionProperty object.
    pub fn register_eval_dynamic_reflection_property(
        &mut self,
        identity: u64,
        declaring_class: &str,
        property_name: &str,
    ) {
        self.register_eval_reflection_property(identity, declaring_class, property_name);
        self.eval_dynamic_reflection_properties.insert(identity);
    }

    /// Returns the declaring class and property name attached to a synthetic ReflectionProperty.
    pub fn eval_reflection_property(&self, identity: u64) -> Option<(&str, &str)> {
        self.eval_reflection_properties
            .get(&identity)
            .map(|(class, property)| (class.as_str(), property.as_str()))
    }

    /// Returns whether a synthetic ReflectionProperty represents a dynamic property.
    pub fn eval_reflection_property_is_dynamic(&self, identity: u64) -> bool {
        self.eval_dynamic_reflection_properties.contains(&identity)
    }

    /// Records reflected class constant or enum case metadata for one synthetic object.
    pub fn register_eval_reflection_class_constant(
        &mut self,
        identity: u64,
        declaring_class: &str,
        constant_name: &str,
        owner_kind: u64,
    ) {
        self.eval_reflection_class_constants.insert(
            identity,
            (
                declaring_class.trim_start_matches('\\').to_string(),
                constant_name.to_string(),
                owner_kind,
            ),
        );
    }

    /// Returns the declaring class, name, and reflection owner kind for a synthetic constant.
    pub fn eval_reflection_class_constant(&self, identity: u64) -> Option<(&str, &str, u64)> {
        self.eval_reflection_class_constants
            .get(&identity)
            .map(|(class, constant, owner_kind)| (class.as_str(), constant.as_str(), *owner_kind))
    }
}
