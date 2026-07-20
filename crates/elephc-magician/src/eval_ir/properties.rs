//! Purpose:
//! Defines eval class properties, hook metadata, defaults, and visibility.
//!
//! Called from:
//! - Class-like parsing, validation, object storage, property access, and Reflection.
//!
//! Key details:
//! - Readonly, promotion, asymmetric set visibility, backing slots, and hooks stay coherent.

use super::*;

/// Public property metadata for a runtime eval class.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalClassProperty {
    name: String,
    trait_origin: Option<String>,
    attributes: Vec<EvalAttribute>,
    property_type: Option<EvalParameterType>,
    set_hook_type: Option<EvalParameterType>,
    visibility: EvalVisibility,
    set_visibility: Option<EvalVisibility>,
    pub(super) is_static: bool,
    is_final: bool,
    pub(super) is_readonly: bool,
    is_promoted: bool,
    is_abstract: bool,
    has_get_hook: bool,
    has_set_hook: bool,
    requires_get_hook: bool,
    requires_set_hook: bool,
    is_virtual: bool,
    default: Option<EvalExpr>,
}

impl EvalClassProperty {
    /// Creates a public eval class property with an optional initializer.
    pub fn new(name: impl Into<String>, default: Option<EvalExpr>) -> Self {
        Self::with_visibility(name, EvalVisibility::Public, default)
    }

    /// Creates an eval class property with explicit PHP visibility.
    pub fn with_visibility(
        name: impl Into<String>,
        visibility: EvalVisibility,
        default: Option<EvalExpr>,
    ) -> Self {
        Self::with_visibility_and_static(name, visibility, false, default)
    }

    /// Creates an eval class property with explicit PHP visibility and static metadata.
    pub fn with_visibility_and_static(
        name: impl Into<String>,
        visibility: EvalVisibility,
        is_static: bool,
        default: Option<EvalExpr>,
    ) -> Self {
        Self::with_visibility_static_and_readonly(name, visibility, is_static, false, default)
    }

    /// Creates an eval class property with explicit storage and readonly metadata.
    pub fn with_visibility_static_and_readonly(
        name: impl Into<String>,
        visibility: EvalVisibility,
        is_static: bool,
        is_readonly: bool,
        default: Option<EvalExpr>,
    ) -> Self {
        Self::with_visibility_static_final_and_readonly(
            name,
            visibility,
            is_static,
            false,
            is_readonly,
            default,
        )
    }

    /// Creates an eval class property with explicit storage and modifier metadata.
    pub fn with_visibility_static_final_and_readonly(
        name: impl Into<String>,
        visibility: EvalVisibility,
        is_static: bool,
        is_final: bool,
        is_readonly: bool,
        default: Option<EvalExpr>,
    ) -> Self {
        Self {
            name: name.into(),
            trait_origin: None,
            attributes: Vec::new(),
            property_type: None,
            set_hook_type: None,
            visibility,
            set_visibility: None,
            is_static,
            is_final,
            is_readonly,
            is_promoted: false,
            is_abstract: false,
            has_get_hook: false,
            has_set_hook: false,
            requires_get_hook: false,
            requires_set_hook: false,
            is_virtual: false,
            default,
        }
    }

    /// Returns a copy of this property marked with concrete get/set hook metadata.
    pub const fn with_hooks(mut self, has_get_hook: bool, has_set_hook: bool) -> Self {
        self.has_get_hook = has_get_hook;
        self.has_set_hook = has_set_hook;
        self.is_virtual = has_get_hook || has_set_hook;
        self
    }

    /// Returns a copy of this property with explicit hook virtuality metadata.
    pub const fn with_virtual(mut self, is_virtual: bool) -> Self {
        self.is_virtual = is_virtual;
        self
    }

    /// Returns a copy of this property marked as an abstract hook contract.
    pub const fn with_abstract_hook_contract(
        mut self,
        requires_get_hook: bool,
        requires_set_hook: bool,
    ) -> Self {
        self.is_abstract = true;
        self.requires_get_hook = requires_get_hook;
        self.requires_set_hook = requires_set_hook;
        self.is_virtual = true;
        self
    }

    /// Returns a copy of this property with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns a copy of this property with its declaring trait retained for magic constants.
    pub fn with_trait_origin(mut self, trait_name: impl Into<String>) -> Self {
        if self.trait_origin.is_none() {
            self.trait_origin = Some(trait_name.into());
        }
        self
    }

    /// Returns a copy of this property with retained type metadata.
    pub fn with_type(mut self, property_type: Option<EvalParameterType>) -> Self {
        self.property_type = property_type;
        self
    }

    /// Returns a copy of this property with retained explicit set-hook parameter type metadata.
    pub fn with_set_hook_type(mut self, set_hook_type: Option<EvalParameterType>) -> Self {
        self.set_hook_type = set_hook_type;
        self
    }

    /// Returns a copy of this property with PHP asymmetric write visibility metadata.
    pub const fn with_set_visibility(mut self, set_visibility: Option<EvalVisibility>) -> Self {
        self.set_visibility = set_visibility;
        self
    }

    /// Returns a copy of this property marked as coming from constructor promotion.
    pub const fn with_promoted(mut self) -> Self {
        self.is_promoted = true;
        self
    }

    /// Returns the PHP-visible property name without `$`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the trait that originally declared this imported property, if any.
    pub fn trait_origin(&self) -> Option<&str> {
        self.trait_origin.as_deref()
    }

    /// Returns attributes declared directly on this class property.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns retained PHP type metadata for this property.
    pub fn property_type(&self) -> Option<&EvalParameterType> {
        self.property_type.as_ref()
    }

    /// Returns retained PHP type metadata for an explicit set-hook parameter.
    pub fn set_hook_type(&self) -> Option<&EvalParameterType> {
        self.set_hook_type.as_ref()
    }

    /// Returns the PHP-visible type accepted by property writes.
    pub fn settable_type(&self) -> Option<&EvalParameterType> {
        self.set_hook_type().or_else(|| self.property_type())
    }

    /// Returns the PHP visibility declared for this property.
    pub const fn visibility(&self) -> EvalVisibility {
        self.visibility
    }

    /// Returns the PHP asymmetric write visibility, if it differs from read visibility.
    pub const fn set_visibility(&self) -> Option<EvalVisibility> {
        self.set_visibility
    }

    /// Returns the visibility that applies to writes for this property.
    pub const fn write_visibility(&self) -> EvalVisibility {
        match self.set_visibility {
            Some(visibility) => visibility,
            None => self.visibility,
        }
    }

    /// Returns whether this property was declared `static`.
    pub const fn is_static(&self) -> bool {
        self.is_static
    }

    /// Returns whether this property was declared `final`.
    pub const fn is_final(&self) -> bool {
        self.is_final
    }

    /// Returns whether this property was declared `readonly`.
    pub const fn is_readonly(&self) -> bool {
        self.is_readonly
    }

    /// Returns whether this property came from constructor property promotion.
    pub const fn is_promoted(&self) -> bool {
        self.is_promoted
    }

    /// Returns whether this property is an abstract property hook contract.
    pub const fn is_abstract(&self) -> bool {
        self.is_abstract
    }

    /// Returns whether this property has a concrete get hook accessor.
    pub const fn has_get_hook(&self) -> bool {
        self.has_get_hook
    }

    /// Returns whether this property has a concrete set hook accessor.
    pub const fn has_set_hook(&self) -> bool {
        self.has_set_hook
    }

    /// Returns whether this abstract property contract requires read access.
    pub const fn requires_get_hook(&self) -> bool {
        self.requires_get_hook
    }

    /// Returns whether this abstract property contract requires write access.
    pub const fn requires_set_hook(&self) -> bool {
        self.requires_set_hook
    }

    /// Returns whether this property is virtual instead of backed by object storage.
    pub const fn is_virtual(&self) -> bool {
        self.is_virtual
    }

    /// Returns the property initializer expression, when one was declared.
    pub fn default(&self) -> Option<&EvalExpr> {
        self.default.as_ref()
    }
}

/// PHP visibility for eval-declared object members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalVisibility {
    Public,
    Protected,
    Private,
}
