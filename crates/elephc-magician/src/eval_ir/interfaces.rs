//! Purpose:
//! Defines eval interfaces, methods, properties, and hook contracts.
//!
//! Called from:
//! - Interface parser, declaration validation, context lookup, and Reflection.
//!
//! Key details:
//! - Parent interfaces and member contracts retain types, visibility, and source metadata.

use super::*;

/// Runtime interface declared by an eval fragment.
#[derive(Debug, Clone)]
pub struct EvalInterface {
    name: String,
    source_location: Option<EvalSourceLocation>,
    parents: Vec<String>,
    attributes: Vec<EvalAttribute>,
    constants: Vec<EvalClassConstant>,
    properties: Vec<EvalInterfaceProperty>,
    methods: Vec<EvalInterfaceMethod>,
}

impl PartialEq for EvalInterface {
    /// Compares interface metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.parents == other.parents
            && self.attributes == other.attributes
            && self.constants == other.constants
            && self.properties == other.properties
            && self.methods == other.methods
    }
}

impl EvalInterface {
    /// Creates a dynamic eval interface with optional parent interfaces and methods.
    pub fn new(
        name: impl Into<String>,
        parents: Vec<String>,
        methods: Vec<EvalInterfaceMethod>,
    ) -> Self {
        Self::with_constants(name, parents, Vec::new(), methods)
    }

    /// Creates a dynamic eval interface with optional parent interfaces, constants, and methods.
    pub fn with_constants(
        name: impl Into<String>,
        parents: Vec<String>,
        constants: Vec<EvalClassConstant>,
        methods: Vec<EvalInterfaceMethod>,
    ) -> Self {
        Self::with_constants_and_properties(name, parents, constants, Vec::new(), methods)
    }

    /// Creates a dynamic eval interface with constants, property contracts, and methods.
    pub fn with_constants_and_properties(
        name: impl Into<String>,
        parents: Vec<String>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalInterfaceProperty>,
        methods: Vec<EvalInterfaceMethod>,
    ) -> Self {
        Self {
            name: name.into(),
            source_location: None,
            parents,
            attributes: Vec::new(),
            constants,
            properties,
            methods,
        }
    }

    /// Returns a copy of this interface with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns a copy of this interface with class-like attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns the original source spelling of this eval-declared interface name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns interface names extended directly by this eval interface.
    pub fn parents(&self) -> &[String] {
        &self.parents
    }

    /// Returns attributes declared directly on this eval interface.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns constants declared directly by this eval interface.
    pub fn constants(&self) -> &[EvalClassConstant] {
        &self.constants
    }

    /// Returns one interface constant by PHP case-sensitive constant name.
    pub fn constant(&self, name: &str) -> Option<&EvalClassConstant> {
        self.constants()
            .iter()
            .find(|constant| constant.name() == name)
    }

    /// Returns property hook contracts declared directly by this eval interface.
    pub fn properties(&self) -> &[EvalInterfaceProperty] {
        &self.properties
    }

    /// Returns method signatures declared directly by this eval interface.
    pub fn methods(&self) -> &[EvalInterfaceMethod] {
        &self.methods
    }
}

/// Property hook contract metadata for a runtime eval interface.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalInterfaceProperty {
    name: String,
    attributes: Vec<EvalAttribute>,
    property_type: Option<EvalParameterType>,
    set_visibility: Option<EvalVisibility>,
    requires_get: bool,
    requires_set: bool,
}

impl EvalInterfaceProperty {
    /// Creates one eval interface property contract.
    pub fn new(name: impl Into<String>, requires_get: bool, requires_set: bool) -> Self {
        Self {
            name: name.into(),
            attributes: Vec::new(),
            property_type: None,
            set_visibility: None,
            requires_get,
            requires_set,
        }
    }

    /// Returns a copy of this interface property with retained type metadata.
    pub fn with_type(mut self, property_type: Option<EvalParameterType>) -> Self {
        self.property_type = property_type;
        self
    }

    /// Returns a copy of this interface property with PHP asymmetric write visibility metadata.
    pub const fn with_set_visibility(mut self, set_visibility: Option<EvalVisibility>) -> Self {
        self.set_visibility = set_visibility;
        self
    }

    /// Returns a copy of this interface property with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns the PHP-visible property name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns attributes declared directly on this interface property.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns retained PHP type metadata for this interface property contract.
    pub fn property_type(&self) -> Option<&EvalParameterType> {
        self.property_type.as_ref()
    }

    /// Returns the PHP asymmetric write visibility declared by this contract, if any.
    pub const fn set_visibility(&self) -> Option<EvalVisibility> {
        self.set_visibility
    }

    /// Returns the visibility required for writes by this property contract.
    pub const fn write_visibility(&self) -> EvalVisibility {
        match self.set_visibility {
            Some(visibility) => visibility,
            None => EvalVisibility::Public,
        }
    }

    /// Returns whether the interface requires the property to be readable.
    pub const fn requires_get(&self) -> bool {
        self.requires_get
    }

    /// Returns whether the interface requires the property to be writable.
    pub const fn requires_set(&self) -> bool {
        self.requires_set
    }

    /// Returns a merged contract containing either side's get/set requirements.
    pub fn merged_with(&self, other: &Self) -> Self {
        Self {
            name: self.name.clone(),
            attributes: self.attributes.clone(),
            property_type: self
                .property_type
                .clone()
                .or_else(|| other.property_type.clone()),
            set_visibility: merge_eval_property_set_visibility(
                self.set_visibility,
                other.set_visibility,
            ),
            requires_get: self.requires_get || other.requires_get,
            requires_set: self.requires_set || other.requires_set,
        }
    }
}

/// Merges interface property set-visibility contracts by keeping the stricter write requirement.
const fn merge_eval_property_set_visibility(
    left: Option<EvalVisibility>,
    right: Option<EvalVisibility>,
) -> Option<EvalVisibility> {
    let left = match left {
        Some(visibility) => visibility,
        None => EvalVisibility::Public,
    };
    let right = match right {
        Some(visibility) => visibility,
        None => EvalVisibility::Public,
    };
    let merged = if eval_visibility_rank(left) < eval_visibility_rank(right) {
        left
    } else {
        right
    };
    match merged {
        EvalVisibility::Public => None,
        visibility => Some(visibility),
    }
}

/// Returns a comparable visibility rank where smaller means more restrictive.
const fn eval_visibility_rank(visibility: EvalVisibility) -> u8 {
    match visibility {
        EvalVisibility::Private => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Public => 3,
    }
}

/// Method signature metadata for a runtime eval interface.
#[derive(Debug, Clone)]
pub struct EvalInterfaceMethod {
    name: String,
    source_location: Option<EvalSourceLocation>,
    attributes: Vec<EvalAttribute>,
    is_static: bool,
    params: Vec<String>,
    parameter_attributes: Vec<Vec<EvalAttribute>>,
    parameter_has_types: Vec<bool>,
    parameter_types: Vec<Option<EvalParameterType>>,
    parameter_defaults: Vec<Option<EvalExpr>>,
    parameter_is_by_ref: Vec<bool>,
    parameter_is_variadic: Vec<bool>,
    return_type: Option<EvalParameterType>,
}

impl PartialEq for EvalInterfaceMethod {
    /// Compares interface method metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.attributes == other.attributes
            && self.is_static == other.is_static
            && self.params == other.params
            && self.parameter_attributes == other.parameter_attributes
            && self.parameter_has_types == other.parameter_has_types
            && self.parameter_types == other.parameter_types
            && self.parameter_defaults == other.parameter_defaults
            && self.parameter_is_by_ref == other.parameter_is_by_ref
            && self.parameter_is_variadic == other.parameter_is_variadic
            && self.return_type == other.return_type
    }
}

impl EvalInterfaceMethod {
    /// Creates one dynamic eval interface method signature.
    pub fn new(name: impl Into<String>, params: Vec<String>) -> Self {
        let parameter_has_types = vec![false; params.len()];
        let parameter_attributes = vec![Vec::new(); params.len()];
        let parameter_types = vec![None; params.len()];
        let parameter_defaults = vec![None; params.len()];
        let parameter_is_by_ref = vec![false; params.len()];
        let parameter_is_variadic = vec![false; params.len()];
        Self {
            name: name.into(),
            source_location: None,
            attributes: Vec::new(),
            is_static: false,
            params,
            parameter_attributes,
            parameter_has_types,
            parameter_types,
            parameter_defaults,
            parameter_is_by_ref,
            parameter_is_variadic,
            return_type: None,
        }
    }

    /// Returns a copy of this interface method with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns a copy of this interface method with its static modifier flag set.
    pub fn with_static(mut self, is_static: bool) -> Self {
        self.is_static = is_static;
        self
    }

    /// Returns a copy of this interface method with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns a copy of this interface method with parameter type-presence flags.
    pub fn with_parameter_type_flags(mut self, parameter_has_types: Vec<bool>) -> Self {
        self.parameter_has_types = parameter_has_types;
        self
    }

    /// Returns a copy of this interface method with source-order parameter attributes.
    pub fn with_parameter_attributes(
        mut self,
        parameter_attributes: Vec<Vec<EvalAttribute>>,
    ) -> Self {
        self.parameter_attributes = parameter_attributes;
        self
    }

    /// Returns a copy of this interface method with source-order parameter type metadata.
    pub fn with_parameter_types(mut self, parameter_types: Vec<Option<EvalParameterType>>) -> Self {
        self.parameter_has_types = parameter_types.iter().map(Option::is_some).collect();
        self.parameter_types = parameter_types;
        self
    }

    /// Returns a copy of this interface method with source-order default expressions.
    pub fn with_parameter_defaults(mut self, parameter_defaults: Vec<Option<EvalExpr>>) -> Self {
        self.parameter_defaults = parameter_defaults;
        self
    }

    /// Returns a copy of this interface method with source-order by-reference flags.
    pub fn with_parameter_by_ref_flags(mut self, parameter_is_by_ref: Vec<bool>) -> Self {
        self.parameter_is_by_ref = parameter_is_by_ref;
        self
    }

    /// Returns a copy of this interface method with source-order variadic flags.
    pub fn with_parameter_variadic_flags(mut self, parameter_is_variadic: Vec<bool>) -> Self {
        self.parameter_is_variadic = parameter_is_variadic;
        self
    }

    /// Returns a copy of this interface method with retained return type metadata.
    pub fn with_return_type(mut self, return_type: Option<EvalParameterType>) -> Self {
        self.return_type = return_type;
        self
    }

    /// Returns the PHP-visible method name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns attributes declared directly on this interface method.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns whether this interface method was declared `static`.
    pub const fn is_static(&self) -> bool {
        self.is_static
    }

    /// Returns source-order parameter names without leading `$`.
    pub fn params(&self) -> &[String] {
        &self.params
    }

    /// Returns source-order parameter attributes.
    pub fn parameter_attributes(&self) -> &[Vec<EvalAttribute>] {
        &self.parameter_attributes
    }

    /// Returns source-order flags for whether each parameter declared a type.
    pub fn parameter_has_types(&self) -> &[bool] {
        &self.parameter_has_types
    }

    /// Returns source-order parameter type metadata.
    pub fn parameter_types(&self) -> &[Option<EvalParameterType>] {
        &self.parameter_types
    }

    /// Returns default expressions declared for each source-order parameter.
    pub fn parameter_defaults(&self) -> &[Option<EvalExpr>] {
        &self.parameter_defaults
    }

    /// Returns source-order flags for whether each parameter was declared by reference.
    pub fn parameter_is_by_ref(&self) -> &[bool] {
        &self.parameter_is_by_ref
    }

    /// Returns source-order flags for whether each parameter was declared variadic.
    pub fn parameter_is_variadic(&self) -> &[bool] {
        &self.parameter_is_variadic
    }

    /// Returns retained return type metadata, if the method declared one.
    pub const fn return_type(&self) -> Option<&EvalParameterType> {
        self.return_type.as_ref()
    }
}
