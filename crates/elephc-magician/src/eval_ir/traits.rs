//! Purpose:
//! Defines eval trait declarations and composed member metadata.
//!
//! Called from:
//! - Trait parser, trait composition, context lookup, and Reflection.
//!
//! Key details:
//! - Nested traits, adaptations, methods, properties, constants, and attributes stay grouped.

use super::*;

/// Runtime trait declared by an eval fragment.
#[derive(Debug, Clone)]
pub struct EvalTrait {
    name: String,
    source_location: Option<EvalSourceLocation>,
    attributes: Vec<EvalAttribute>,
    traits: Vec<String>,
    trait_adaptations: Vec<EvalTraitAdaptation>,
    constants: Vec<EvalClassConstant>,
    properties: Vec<EvalClassProperty>,
    methods: Vec<EvalClassMethod>,
}

impl PartialEq for EvalTrait {
    /// Compares trait metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.attributes == other.attributes
            && self.traits == other.traits
            && self.trait_adaptations == other.trait_adaptations
            && self.constants == other.constants
            && self.properties == other.properties
            && self.methods == other.methods
    }
}

impl EvalTrait {
    /// Creates a dynamic eval trait with public properties and methods.
    pub fn new(
        name: impl Into<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_constants(name, Vec::new(), properties, methods)
    }

    /// Creates a dynamic eval trait with constants, properties, and methods.
    pub fn with_constants(
        name: impl Into<String>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_constants_traits_adaptations(
            name,
            constants,
            properties,
            methods,
            Vec::new(),
            Vec::new(),
        )
    }

    /// Creates a dynamic eval trait with trait uses, adaptations, constants, properties, and methods.
    pub fn with_constants_traits_adaptations(
        name: impl Into<String>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
        traits: Vec<String>,
        trait_adaptations: Vec<EvalTraitAdaptation>,
    ) -> Self {
        Self {
            name: name.into(),
            source_location: None,
            attributes: Vec::new(),
            traits,
            trait_adaptations,
            constants,
            properties,
            methods,
        }
    }

    /// Returns a copy of this trait with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns a copy of this trait with class-like attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns the original source spelling of this eval-declared trait name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns attributes declared directly on this eval trait.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns trait names used directly by this eval trait.
    pub fn traits(&self) -> &[String] {
        &self.traits
    }

    /// Returns trait adaptations declared directly by this eval trait.
    pub fn trait_adaptations(&self) -> &[EvalTraitAdaptation] {
        &self.trait_adaptations
    }

    /// Returns constants declared directly by this eval trait.
    pub fn constants(&self) -> &[EvalClassConstant] {
        &self.constants
    }

    /// Returns one trait constant by PHP case-sensitive constant name.
    pub fn constant(&self, name: &str) -> Option<&EvalClassConstant> {
        self.constants()
            .iter()
            .find(|constant| constant.name() == name)
    }

    /// Returns public properties declared directly by this eval trait.
    pub fn properties(&self) -> &[EvalClassProperty] {
        &self.properties
    }

    /// Returns public methods declared directly by this eval trait.
    pub fn methods(&self) -> &[EvalClassMethod] {
        &self.methods
    }
}
