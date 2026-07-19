//! Purpose:
//! Defines eval enum declarations, backing types, and case metadata.
//!
//! Called from:
//! - Enum parser, declaration execution, context lookup, and Reflection.
//!
//! Key details:
//! - Unit/backed cases, traits, interfaces, methods, and source metadata stay together.

use super::*;

/// Runtime enum declared by an eval fragment.
#[derive(Debug, Clone)]
pub struct EvalEnum {
    name: String,
    source_location: Option<EvalSourceLocation>,
    backing_type: Option<EvalEnumBackingType>,
    interfaces: Vec<String>,
    attributes: Vec<EvalAttribute>,
    traits: Vec<String>,
    trait_adaptations: Vec<EvalTraitAdaptation>,
    cases: Vec<EvalEnumCase>,
    constants: Vec<EvalClassConstant>,
    methods: Vec<EvalClassMethod>,
}

impl PartialEq for EvalEnum {
    /// Compares enum metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.backing_type == other.backing_type
            && self.interfaces == other.interfaces
            && self.attributes == other.attributes
            && self.traits == other.traits
            && self.trait_adaptations == other.trait_adaptations
            && self.cases == other.cases
            && self.constants == other.constants
            && self.methods == other.methods
    }
}

impl EvalEnum {
    /// Creates a dynamic eval enum with cases and optional backing type.
    pub fn new(
        name: impl Into<String>,
        backing_type: Option<EvalEnumBackingType>,
        cases: Vec<EvalEnumCase>,
    ) -> Self {
        Self::with_members(
            name,
            backing_type,
            Vec::new(),
            cases,
            Vec::new(),
            Vec::new(),
        )
    }

    /// Creates a dynamic eval enum with interfaces, cases, constants, and methods.
    pub fn with_members(
        name: impl Into<String>,
        backing_type: Option<EvalEnumBackingType>,
        interfaces: Vec<String>,
        cases: Vec<EvalEnumCase>,
        constants: Vec<EvalClassConstant>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_members_traits_adaptations(
            name,
            backing_type,
            interfaces,
            cases,
            constants,
            methods,
            Vec::new(),
            Vec::new(),
        )
    }

    /// Creates a dynamic eval enum with traits, adaptations, cases, constants, and methods.
    pub fn with_members_traits_adaptations(
        name: impl Into<String>,
        backing_type: Option<EvalEnumBackingType>,
        interfaces: Vec<String>,
        cases: Vec<EvalEnumCase>,
        constants: Vec<EvalClassConstant>,
        methods: Vec<EvalClassMethod>,
        traits: Vec<String>,
        trait_adaptations: Vec<EvalTraitAdaptation>,
    ) -> Self {
        Self {
            name: name.into(),
            source_location: None,
            backing_type,
            interfaces,
            attributes: Vec::new(),
            traits,
            trait_adaptations,
            cases,
            constants,
            methods,
        }
    }

    /// Returns a copy of this enum with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns a copy of this enum with class-like attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns the original source spelling of this eval-declared enum name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns the optional scalar backing type for this enum.
    pub const fn backing_type(&self) -> Option<EvalEnumBackingType> {
        self.backing_type
    }

    /// Returns interface names implemented directly by this eval enum.
    pub fn interfaces(&self) -> &[String] {
        &self.interfaces
    }

    /// Returns attributes declared directly on this eval enum.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns trait names used directly by this eval enum.
    pub fn traits(&self) -> &[String] {
        &self.traits
    }

    /// Returns trait adaptations declared directly by this eval enum.
    pub fn trait_adaptations(&self) -> &[EvalTraitAdaptation] {
        &self.trait_adaptations
    }

    /// Returns cases declared directly by this eval enum.
    pub fn cases(&self) -> &[EvalEnumCase] {
        &self.cases
    }

    /// Returns one enum case by PHP case-sensitive case name.
    pub fn case(&self, name: &str) -> Option<&EvalEnumCase> {
        self.cases().iter().find(|case| case.name() == name)
    }

    /// Returns constants declared directly by this eval enum.
    pub fn constants(&self) -> &[EvalClassConstant] {
        &self.constants
    }

    /// Returns methods declared directly by this eval enum.
    pub fn methods(&self) -> &[EvalClassMethod] {
        &self.methods
    }

    /// Builds class-shaped metadata used for enum method and relation dispatch.
    pub fn as_class_metadata(&self) -> EvalClass {
        EvalClass::with_modifiers_traits_adaptations_and_constants(
            self.name.clone(),
            false,
            true,
            None,
            self.interfaces.clone(),
            self.traits.clone(),
            self.trait_adaptations.clone(),
            self.constants.clone(),
            Vec::new(),
            self.methods.clone(),
        )
        .with_attributes(self.attributes.clone())
        .with_source_location_option(self.source_location)
    }
}

/// Scalar backing type for a runtime eval enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalEnumBackingType {
    Int,
    String,
}

/// One case declared by a runtime eval enum.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalEnumCase {
    name: String,
    attributes: Vec<EvalAttribute>,
    value: Option<EvalExpr>,
}

impl EvalEnumCase {
    /// Creates an eval enum case with an optional backing value expression.
    pub fn new(name: impl Into<String>, value: Option<EvalExpr>) -> Self {
        Self {
            name: name.into(),
            attributes: Vec::new(),
            value,
        }
    }

    /// Returns a copy of this enum case with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns the PHP-visible enum case name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns attributes declared directly on this enum case.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns the optional backing value expression.
    pub fn value(&self) -> Option<&EvalExpr> {
        self.value.as_ref()
    }
}
