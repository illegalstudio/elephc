//! Purpose:
//! Defines eval class methods and their complete callable metadata.
//!
//! Called from:
//! - Class-like parsing, validation, dynamic invocation, closures, and Reflection.
//!
//! Key details:
//! - Parameters, statics, visibility, hook bodies, return types, and source metadata remain aligned.

use super::*;

/// Public method metadata for a runtime eval class.
#[derive(Debug, Clone)]
pub struct EvalClassMethod {
    name: String,
    trait_origin: Option<String>,
    trait_origin_method: Option<String>,
    source_location: Option<EvalSourceLocation>,
    attributes: Vec<EvalAttribute>,
    visibility: EvalVisibility,
    is_static: bool,
    is_abstract: bool,
    is_final: bool,
    params: Vec<String>,
    parameter_attributes: Vec<Vec<EvalAttribute>>,
    parameter_has_types: Vec<bool>,
    parameter_types: Vec<Option<EvalParameterType>>,
    parameter_defaults: Vec<Option<EvalExpr>>,
    parameter_is_by_ref: Vec<bool>,
    parameter_is_variadic: Vec<bool>,
    return_type: Option<EvalParameterType>,
    body: Vec<EvalStmt>,
}

impl PartialEq for EvalClassMethod {
    /// Compares class method metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.trait_origin == other.trait_origin
            && self.trait_origin_method == other.trait_origin_method
            && self.attributes == other.attributes
            && self.visibility == other.visibility
            && self.is_static == other.is_static
            && self.is_abstract == other.is_abstract
            && self.is_final == other.is_final
            && self.params == other.params
            && self.parameter_attributes == other.parameter_attributes
            && self.parameter_has_types == other.parameter_has_types
            && self.parameter_types == other.parameter_types
            && self.parameter_defaults == other.parameter_defaults
            && self.parameter_is_by_ref == other.parameter_is_by_ref
            && self.parameter_is_variadic == other.parameter_is_variadic
            && self.return_type == other.return_type
            && self.body == other.body
    }
}

impl EvalClassMethod {
    /// Creates a public eval class method with source-order parameters and body.
    pub fn new(name: impl Into<String>, params: Vec<String>, body: Vec<EvalStmt>) -> Self {
        Self::with_modifiers(name, false, false, params, body)
    }

    /// Creates a public eval class method with optional abstract/final modifiers.
    pub fn with_modifiers(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        params: Vec<String>,
        body: Vec<EvalStmt>,
    ) -> Self {
        Self::with_visibility_and_modifiers(
            name,
            EvalVisibility::Public,
            false,
            is_abstract,
            is_final,
            params,
            body,
        )
    }

    /// Creates an eval class method with explicit visibility and optional modifiers.
    pub fn with_visibility_and_modifiers(
        name: impl Into<String>,
        visibility: EvalVisibility,
        is_static: bool,
        is_abstract: bool,
        is_final: bool,
        params: Vec<String>,
        body: Vec<EvalStmt>,
    ) -> Self {
        let parameter_has_types = vec![false; params.len()];
        let parameter_attributes = vec![Vec::new(); params.len()];
        let parameter_types = vec![None; params.len()];
        let parameter_defaults = vec![None; params.len()];
        let parameter_is_by_ref = vec![false; params.len()];
        let parameter_is_variadic = vec![false; params.len()];
        Self {
            name: name.into(),
            trait_origin: None,
            trait_origin_method: None,
            source_location: None,
            attributes: Vec::new(),
            visibility,
            is_static,
            is_abstract,
            is_final,
            params,
            parameter_attributes,
            parameter_has_types,
            parameter_types,
            parameter_defaults,
            parameter_is_by_ref,
            parameter_is_variadic,
            return_type: None,
            body,
        }
    }

    /// Returns a copy of this method with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns the PHP-visible method name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns a copy of this method with its declaring trait retained for magic constants.
    pub fn with_trait_origin(mut self, trait_name: impl Into<String>) -> Self {
        if self.trait_origin.is_none() {
            self.trait_origin = Some(trait_name.into());
            self.trait_origin_method = Some(self.name.clone());
        }
        self
    }

    /// Returns the trait that originally declared this imported method, if any.
    pub fn trait_origin(&self) -> Option<&str> {
        self.trait_origin.as_deref()
    }

    /// Returns the PHP `__FUNCTION__` value for this method body.
    pub fn magic_function_name(&self) -> &str {
        self.trait_origin_method.as_deref().unwrap_or(&self.name)
    }

    /// Returns the PHP `__METHOD__` value for this method body.
    pub fn magic_method_name(&self, class_name: &str) -> String {
        let owner = self.trait_origin().unwrap_or(class_name);
        format!(
            "{}::{}",
            owner.trim_start_matches('\\'),
            self.magic_function_name()
        )
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns a copy of this method with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns a copy of this method with source-order parameter type-presence flags.
    pub fn with_parameter_type_flags(mut self, parameter_has_types: Vec<bool>) -> Self {
        self.parameter_has_types = parameter_has_types;
        self
    }

    /// Returns a copy of this method with source-order parameter attributes.
    pub fn with_parameter_attributes(
        mut self,
        parameter_attributes: Vec<Vec<EvalAttribute>>,
    ) -> Self {
        self.parameter_attributes = parameter_attributes;
        self
    }

    /// Returns a copy of this method with source-order parameter type metadata.
    pub fn with_parameter_types(mut self, parameter_types: Vec<Option<EvalParameterType>>) -> Self {
        self.parameter_has_types = parameter_types.iter().map(Option::is_some).collect();
        self.parameter_types = parameter_types;
        self
    }

    /// Returns a copy of this method with source-order default expressions.
    pub fn with_parameter_defaults(mut self, parameter_defaults: Vec<Option<EvalExpr>>) -> Self {
        self.parameter_defaults = parameter_defaults;
        self
    }

    /// Returns a copy of this method with source-order by-reference flags.
    pub fn with_parameter_by_ref_flags(mut self, parameter_is_by_ref: Vec<bool>) -> Self {
        self.parameter_is_by_ref = parameter_is_by_ref;
        self
    }

    /// Returns a copy of this method with source-order variadic flags.
    pub fn with_parameter_variadic_flags(mut self, parameter_is_variadic: Vec<bool>) -> Self {
        self.parameter_is_variadic = parameter_is_variadic;
        self
    }

    /// Returns a copy of this method with retained return type metadata.
    pub fn with_return_type(mut self, return_type: Option<EvalParameterType>) -> Self {
        self.return_type = return_type;
        self
    }

    /// Returns attributes declared directly on this class method.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns a copy of this method with a different PHP-visible name.
    pub fn renamed(&self, name: impl Into<String>) -> Self {
        let mut method = self.clone();
        method.name = name.into();
        method
    }

    /// Returns a copy of this method with a different PHP visibility.
    pub fn with_visibility_override(&self, visibility: EvalVisibility) -> Self {
        let mut method = self.clone();
        method.visibility = visibility;
        method
    }

    /// Returns the PHP visibility declared for this method.
    pub const fn visibility(&self) -> EvalVisibility {
        self.visibility
    }

    /// Returns whether this method was declared `static`.
    pub const fn is_static(&self) -> bool {
        self.is_static
    }

    /// Returns whether this eval-declared method was declared `abstract`.
    pub const fn is_abstract(&self) -> bool {
        self.is_abstract
    }

    /// Returns whether this eval-declared method was declared `final`.
    pub const fn is_final(&self) -> bool {
        self.is_final
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

    /// Returns the dynamic EvalIR statements that form the method body.
    pub fn body(&self) -> &[EvalStmt] {
        &self.body
    }
}
