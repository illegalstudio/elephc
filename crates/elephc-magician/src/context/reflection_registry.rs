//! Purpose:
//! Registers synthetic Reflection owner identities and their eval metadata targets.
//!
//! Called from:
//! - Reflection object construction and reflected method dispatch.
//!
//! Key details:
//! - Class, function, method, property, constant, and closure targets use stable runtime identities.

use super::*;

/// The kind of reflector one object identity is registered as.
///
/// One address holds one live object, so it can be only one kind of reflector at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvalReflectionKind {
    Attribute,
    Class,
    Function,
    Method,
    Property,
    ClassConstant,
}

impl ElephcEvalContext {
    /// Drops every registration under `identity` that is NOT of kind `keep`.
    ///
    /// The per-kind tables are keyed by object ADDRESS and a compiled free never reaches them, so
    /// a reflector allocated where a dead reflector of another kind lived inherited its entry.
    /// The class table answers `getFileName()` for any identity it holds, so a `ReflectionMethod`
    /// built where a freed `ReflectionObject` of the calling class had been reported THAT class's
    /// file: the Symfony demo showed `SourceCodeExtension.php` as the controller's source, with the
    /// method's own lines sliced out of it. Registering a reflector, or re-binding one on a call,
    /// clears the other kinds first.
    pub fn forget_eval_reflection_identity_except(
        &mut self,
        identity: u64,
        keep: EvalReflectionKind,
    ) {
        if keep != EvalReflectionKind::Attribute {
            self.eval_reflection_attributes.remove(&identity);
        }
        if keep != EvalReflectionKind::Class {
            self.eval_reflection_classes.remove(&identity);
        }
        if keep != EvalReflectionKind::Function {
            self.eval_reflection_functions.remove(&identity);
            self.eval_reflection_function_closure_targets.remove(&identity);
        }
        if keep != EvalReflectionKind::Method {
            self.eval_reflection_methods.remove(&identity);
        }
        if keep != EvalReflectionKind::Property {
            self.eval_reflection_properties.remove(&identity);
            self.eval_dynamic_reflection_properties.remove(&identity);
        }
        if keep != EvalReflectionKind::ClassConstant {
            self.eval_reflection_class_constants.remove(&identity);
        }
    }

    /// Drops every reflection registration held under one object identity.
    ///
    /// The registry is keyed by object ADDRESS and outlives the reflector: a compiled free never
    /// reaches it. Once that address holds an object that is not a reflector at all, each entry
    /// still filed under it is a dead reflector's, and left in place it answers that object's
    /// calls -- a console command reported the name of a `ReflectionMethod` freed from the same
    /// block. Returns whether anything was dropped.
    pub fn forget_eval_reflection_identity(&mut self, identity: u64) -> bool {
        let mut dropped = self.eval_reflection_attributes.remove(&identity).is_some();
        dropped |= self.eval_reflection_classes.remove(&identity).is_some();
        dropped |= self.eval_reflection_functions.remove(&identity).is_some();
        dropped |= self.eval_reflection_function_closure_targets.remove(&identity).is_some();
        dropped |= self.eval_reflection_methods.remove(&identity).is_some();
        dropped |= self.eval_reflection_properties.remove(&identity).is_some();
        dropped |= self.eval_dynamic_reflection_properties.remove(&identity);
        dropped |= self.eval_reflection_class_constants.remove(&identity).is_some();
        dropped
    }

    /// Records eval-declared attribute metadata for one synthetic ReflectionAttribute object.
    pub fn register_eval_reflection_attribute(
        &mut self,
        identity: u64,
        attribute: EvalAttribute,
        target: u64,
        repeated: bool,
    ) {
        self.forget_eval_reflection_identity_except(identity, EvalReflectionKind::Attribute);
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
        self.forget_eval_reflection_identity_except(identity, EvalReflectionKind::Class);
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
        self.forget_eval_reflection_identity_except(identity, EvalReflectionKind::Function);
        // A callable target is attached AFTER the function is registered; one still filed here
        // is a freed ReflectionFunction's.
        self.eval_reflection_function_closure_targets.remove(&identity);
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
        self.forget_eval_reflection_identity_except(identity, EvalReflectionKind::Function);
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
        self.forget_eval_reflection_identity_except(identity, EvalReflectionKind::Method);
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
        self.forget_eval_reflection_identity_except(identity, EvalReflectionKind::Property);
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
        self.forget_eval_reflection_identity_except(identity, EvalReflectionKind::ClassConstant);
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
