//! Purpose:
//! Defines eval classes, trait adaptations, and class constants.
//!
//! Called from:
//! - Class parser, declaration validation, context lookup, object construction, and Reflection.
//!
//! Key details:
//! - Parent/interface/trait composition and member lists remain one class declaration model.

use super::*;

/// Runtime class declared by an eval fragment.
#[derive(Debug, Clone)]
pub struct EvalClass {
    name: String,
    source_location: Option<EvalSourceLocation>,
    is_abstract: bool,
    is_final: bool,
    is_readonly_class: bool,
    is_anonymous: bool,
    parent: Option<String>,
    interfaces: Vec<String>,
    attributes: Vec<EvalAttribute>,
    traits: Vec<String>,
    trait_adaptations: Vec<EvalTraitAdaptation>,
    constants: Vec<EvalClassConstant>,
    properties: Vec<EvalClassProperty>,
    methods: Vec<EvalClassMethod>,
}

impl PartialEq for EvalClass {
    /// Compares class metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.is_abstract == other.is_abstract
            && self.is_final == other.is_final
            && self.is_readonly_class == other.is_readonly_class
            && self.is_anonymous == other.is_anonymous
            && self.parent == other.parent
            && self.interfaces == other.interfaces
            && self.attributes == other.attributes
            && self.traits == other.traits
            && self.trait_adaptations == other.trait_adaptations
            && self.constants == other.constants
            && self.properties == other.properties
            && self.methods == other.methods
    }
}

impl EvalClass {
    /// Creates a dynamic eval class with public properties and methods, and no relations.
    pub fn new(
        name: impl Into<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_modifiers(name, false, false, None, Vec::new(), properties, methods)
    }

    /// Creates a dynamic eval class with optional parent and implemented interfaces.
    pub fn with_relations(
        name: impl Into<String>,
        parent: Option<String>,
        interfaces: Vec<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_modifiers(name, false, false, parent, interfaces, properties, methods)
    }

    /// Creates a dynamic eval class with optional modifiers, parent, and interfaces.
    pub fn with_modifiers(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_class_modifiers(
            name,
            is_abstract,
            is_final,
            false,
            parent,
            interfaces,
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with class modifiers, optional parent, and interfaces.
    pub fn with_class_modifiers(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        is_readonly_class: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_class_modifiers_and_traits(
            name,
            is_abstract,
            is_final,
            is_readonly_class,
            parent,
            interfaces,
            Vec::new(),
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with optional modifiers, relations, and trait uses.
    pub fn with_modifiers_and_traits(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_class_modifiers_and_traits(
            name,
            is_abstract,
            is_final,
            false,
            parent,
            interfaces,
            traits,
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with class modifiers, relations, and trait uses.
    pub fn with_class_modifiers_and_traits(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        is_readonly_class: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_class_modifiers_traits_and_constants(
            name,
            is_abstract,
            is_final,
            is_readonly_class,
            parent,
            interfaces,
            traits,
            Vec::new(),
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with modifiers, relations, trait uses, constants, and members.
    pub fn with_modifiers_traits_and_constants(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_class_modifiers_traits_and_constants(
            name,
            is_abstract,
            is_final,
            false,
            parent,
            interfaces,
            traits,
            constants,
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with class modifiers, relations, trait uses, constants, and members.
    pub fn with_class_modifiers_traits_and_constants(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        is_readonly_class: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_class_modifiers_traits_adaptations_and_constants(
            name,
            is_abstract,
            is_final,
            is_readonly_class,
            parent,
            interfaces,
            traits,
            Vec::new(),
            constants,
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with modifiers, relations, trait adaptations, constants, and members.
    pub fn with_modifiers_traits_adaptations_and_constants(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        trait_adaptations: Vec<EvalTraitAdaptation>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_class_modifiers_traits_adaptations_and_constants(
            name,
            is_abstract,
            is_final,
            false,
            parent,
            interfaces,
            traits,
            trait_adaptations,
            constants,
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with all class modifiers, relations, adaptations, constants, and members.
    pub fn with_class_modifiers_traits_adaptations_and_constants(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        is_readonly_class: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        trait_adaptations: Vec<EvalTraitAdaptation>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self {
            name: name.into(),
            source_location: None,
            is_abstract,
            is_final,
            is_readonly_class,
            is_anonymous: false,
            parent,
            interfaces,
            attributes: Vec::new(),
            traits,
            trait_adaptations,
            constants,
            properties,
            methods,
        }
    }

    /// Returns a copy of this class with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns a copy of this class with optional source-location metadata attached.
    pub const fn with_source_location_option(
        mut self,
        source_location: Option<EvalSourceLocation>,
    ) -> Self {
        self.source_location = source_location;
        self
    }

    /// Returns a copy of this class with class-like attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Marks this eval class metadata as an anonymous class expression result.
    pub fn with_anonymous(mut self) -> Self {
        self.is_anonymous = true;
        self
    }

    /// Marks instance properties readonly when this metadata represents a `readonly class`.
    pub fn with_readonly_properties(mut self) -> Self {
        if self.is_readonly_class {
            for property in &mut self.properties {
                if !property.is_static {
                    property.is_readonly = true;
                }
            }
        }
        self
    }

    /// Returns the original source spelling of this eval-declared class name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns whether this eval-declared class was declared `abstract`.
    pub const fn is_abstract(&self) -> bool {
        self.is_abstract
    }

    /// Returns whether this eval-declared class was declared `final`.
    pub const fn is_final(&self) -> bool {
        self.is_final
    }

    /// Returns whether this eval-declared class was declared `readonly`.
    pub const fn is_readonly_class(&self) -> bool {
        self.is_readonly_class
    }

    /// Returns whether this eval class came from a `new class {}` expression.
    pub const fn is_anonymous(&self) -> bool {
        self.is_anonymous
    }

    /// Returns the parent class name declared by this eval class, when present.
    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    /// Returns interface names implemented directly by this eval class.
    pub fn interfaces(&self) -> &[String] {
        &self.interfaces
    }

    /// Returns attributes declared directly on this eval class.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns trait names used directly by this eval class.
    pub fn traits(&self) -> &[String] {
        &self.traits
    }

    /// Returns trait adaptations declared on this eval class.
    pub fn trait_adaptations(&self) -> &[EvalTraitAdaptation] {
        &self.trait_adaptations
    }

    /// Returns class constants declared directly by this eval class.
    pub fn constants(&self) -> &[EvalClassConstant] {
        &self.constants
    }

    /// Returns public properties declared directly by this eval class.
    pub fn properties(&self) -> &[EvalClassProperty] {
        &self.properties
    }

    /// Returns public methods declared directly by this eval class.
    pub fn methods(&self) -> &[EvalClassMethod] {
        &self.methods
    }

    /// Returns a public method by PHP case-insensitive method name.
    pub fn method(&self, name: &str) -> Option<&EvalClassMethod> {
        self.methods()
            .iter()
            .find(|method| method.name().eq_ignore_ascii_case(name))
    }

    /// Returns a class constant by PHP case-sensitive constant name.
    pub fn constant(&self, name: &str) -> Option<&EvalClassConstant> {
        self.constants()
            .iter()
            .find(|constant| constant.name() == name)
    }
}

/// Adaptation rule declared in a runtime eval class `use Trait { ... }` block.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalTraitAdaptation {
    Alias {
        trait_name: Option<String>,
        method: String,
        alias: Option<String>,
        visibility: Option<EvalVisibility>,
    },
    InsteadOf {
        trait_name: Option<String>,
        method: String,
        instead_of: Vec<String>,
    },
}

/// Constant metadata for a runtime eval class.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalClassConstant {
    name: String,
    trait_origin: Option<String>,
    attributes: Vec<EvalAttribute>,
    visibility: EvalVisibility,
    is_final: bool,
    value: EvalExpr,
}

impl EvalClassConstant {
    /// Creates a public eval class constant with a value expression.
    pub fn new(name: impl Into<String>, value: EvalExpr) -> Self {
        Self::with_visibility(name, EvalVisibility::Public, value)
    }

    /// Creates an eval class constant with explicit PHP visibility.
    pub fn with_visibility(
        name: impl Into<String>,
        visibility: EvalVisibility,
        value: EvalExpr,
    ) -> Self {
        Self::with_visibility_and_final(name, visibility, false, value)
    }

    /// Creates an eval class constant with explicit PHP visibility and finality.
    pub fn with_visibility_and_final(
        name: impl Into<String>,
        visibility: EvalVisibility,
        is_final: bool,
        value: EvalExpr,
    ) -> Self {
        Self {
            name: name.into(),
            trait_origin: None,
            attributes: Vec::new(),
            visibility,
            is_final,
            value,
        }
    }

    /// Returns a copy of this class constant with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns a copy of this constant with its declaring trait retained for magic constants.
    pub fn with_trait_origin(mut self, trait_name: impl Into<String>) -> Self {
        if self.trait_origin.is_none() {
            self.trait_origin = Some(trait_name.into());
        }
        self
    }

    /// Returns the PHP-visible class constant name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the trait that originally declared this imported constant, if any.
    pub fn trait_origin(&self) -> Option<&str> {
        self.trait_origin.as_deref()
    }

    /// Returns attributes declared directly on this class constant.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns the PHP visibility declared for this constant.
    pub const fn visibility(&self) -> EvalVisibility {
        self.visibility
    }

    /// Returns whether this class constant was declared `final`.
    pub const fn is_final(&self) -> bool {
        self.is_final
    }

    /// Returns the constant initializer expression.
    pub fn value(&self) -> &EvalExpr {
        &self.value
    }
}
