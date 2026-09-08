//! Purpose:
//! Defines closure captures, functions, and callable parameter type metadata.
//!
//! Called from:
//! - Function/closure parsing, dynamic binding, type checks, and Reflection.
//!
//! Key details:
//! - Parameter names, types, defaults, by-ref flags, and variadics remain index-aligned.

use super::*;

/// One lexical variable captured by a runtime eval closure literal.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalClosureCapture {
    name: String,
    by_ref: bool,
}

impl EvalClosureCapture {
    /// Creates one closure capture with its source variable name and reference mode.
    pub fn new(name: impl Into<String>, by_ref: bool) -> Self {
        Self {
            name: name.into(),
            by_ref,
        }
    }

    /// Returns the captured variable name without the leading `$`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns whether this capture was declared as `use (&$name)`.
    pub const fn by_ref(&self) -> bool {
        self.by_ref
    }
}

/// Runtime user function declared by an eval fragment.
#[derive(Debug, Clone)]
pub struct EvalFunction {
    name: String,
    display_name: String,
    source_location: Option<EvalSourceLocation>,
    strict_types: bool,
    attributes: Vec<EvalAttribute>,
    params: Vec<String>,
    parameter_attributes: Vec<Vec<EvalAttribute>>,
    parameter_types: Vec<Option<EvalParameterType>>,
    parameter_defaults: Vec<Option<EvalExpr>>,
    parameter_is_by_ref: Vec<bool>,
    parameter_is_variadic: Vec<bool>,
    return_type: Option<EvalParameterType>,
    body: Vec<EvalStmt>,
}

impl PartialEq for EvalFunction {
    /// Compares function metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.display_name == other.display_name
            && self.strict_types == other.strict_types
            && self.attributes == other.attributes
            && self.params == other.params
            && self.parameter_attributes == other.parameter_attributes
            && self.parameter_types == other.parameter_types
            && self.parameter_defaults == other.parameter_defaults
            && self.parameter_is_by_ref == other.parameter_is_by_ref
            && self.parameter_is_variadic == other.parameter_is_variadic
            && self.return_type == other.return_type
            && self.body == other.body
    }
}

impl EvalFunction {
    /// Creates a dynamic eval function with source-order parameters and body.
    pub fn new(name: impl Into<String>, params: Vec<String>, body: Vec<EvalStmt>) -> Self {
        let parameter_attributes = vec![Vec::new(); params.len()];
        let parameter_types = vec![None; params.len()];
        let parameter_defaults = vec![None; params.len()];
        let parameter_is_by_ref = vec![false; params.len()];
        let parameter_is_variadic = vec![false; params.len()];
        let name = name.into();
        Self {
            display_name: name.clone(),
            name,
            source_location: None,
            strict_types: false,
            attributes: Vec::new(),
            params,
            parameter_attributes,
            parameter_types,
            parameter_defaults,
            parameter_is_by_ref,
            parameter_is_variadic,
            return_type: None,
            body,
        }
    }

    /// Returns a copy of this function with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Retains the strict-types mode of the compilation unit that owns this body.
    pub const fn with_strict_types(mut self, strict_types: bool) -> Self {
        self.strict_types = strict_types;
        self
    }

    /// Attaches the PHP-visible callable identity used in runtime diagnostics.
    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = display_name.into();
        self
    }

    /// Returns a copy of this function with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns a copy of this function with source-order parameter attributes.
    pub fn with_parameter_attributes(
        mut self,
        parameter_attributes: Vec<Vec<EvalAttribute>>,
    ) -> Self {
        self.parameter_attributes = parameter_attributes;
        self
    }

    /// Returns a copy of this function with source-order parameter type metadata.
    pub fn with_parameter_types(mut self, parameter_types: Vec<Option<EvalParameterType>>) -> Self {
        self.parameter_types = parameter_types;
        self
    }

    /// Returns a copy of this function with source-order default expressions.
    pub fn with_parameter_defaults(mut self, parameter_defaults: Vec<Option<EvalExpr>>) -> Self {
        self.parameter_defaults = parameter_defaults;
        self
    }

    /// Returns a copy of this function with source-order by-reference flags.
    pub fn with_parameter_by_ref_flags(mut self, parameter_is_by_ref: Vec<bool>) -> Self {
        self.parameter_is_by_ref = parameter_is_by_ref;
        self
    }

    /// Returns a copy of this function with source-order variadic flags.
    pub fn with_parameter_variadic_flags(mut self, parameter_is_variadic: Vec<bool>) -> Self {
        self.parameter_is_variadic = parameter_is_variadic;
        self
    }

    /// Returns a copy of this function with retained return type metadata.
    pub fn with_return_type(mut self, return_type: Option<EvalParameterType>) -> Self {
        self.return_type = return_type;
        self
    }

    /// Returns the original source spelling of this eval-declared function name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the PHP-visible callable identity used in runtime diagnostics.
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns the lexical strict-types mode used while this function body executes.
    pub const fn strict_types(&self) -> bool {
        self.strict_types
    }

    /// Returns attributes declared directly on this eval function.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns source-order parameter names without leading `$`.
    pub fn params(&self) -> &[String] {
        &self.params
    }

    /// Returns source-order parameter attributes.
    pub fn parameter_attributes(&self) -> &[Vec<EvalAttribute>] {
        &self.parameter_attributes
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

    /// Returns PHP's last-required positional count for this function signature.
    pub fn required_num_args(&self) -> usize {
        let fixed_count = self
            .parameter_is_variadic
            .iter()
            .position(|is_variadic| *is_variadic)
            .unwrap_or(self.params.len());
        (0..fixed_count)
            .rfind(|index| !self.parameter_defaults.get(*index).is_some_and(Option::is_some))
            .map_or(0, |index| index + 1)
    }

    /// Returns retained return type metadata, if the function declared one.
    pub const fn return_type(&self) -> Option<&EvalParameterType> {
        self.return_type.as_ref()
    }

    /// Returns the dynamic EvalIR statements that form the function body.
    pub fn body(&self) -> &[EvalStmt] {
        &self.body
    }
}

/// One supported eval method parameter type atom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalParameterTypeVariant {
    Array,
    Bool,
    Callable,
    Class(String),
    Float,
    Int,
    Iterable,
    Mixed,
    Never,
    Object,
    String,
    Void,
}

/// How multiple eval parameter type atoms combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalParameterTypeKind {
    Union,
    Intersection,
}

/// Type metadata retained for one eval method parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalParameterType {
    variants: Vec<EvalParameterTypeVariant>,
    allows_null: bool,
    kind: EvalParameterTypeKind,
}

impl EvalParameterType {
    /// Creates one eval method parameter type from union variants and nullability.
    pub fn new(variants: Vec<EvalParameterTypeVariant>, allows_null: bool) -> Self {
        Self {
            variants,
            allows_null,
            kind: EvalParameterTypeKind::Union,
        }
    }

    /// Creates one eval method parameter type from intersection variants.
    pub fn intersection(variants: Vec<EvalParameterTypeVariant>) -> Self {
        Self {
            variants,
            allows_null: false,
            kind: EvalParameterTypeKind::Intersection,
        }
    }

    /// Returns the non-null type atoms in source order.
    pub fn variants(&self) -> &[EvalParameterTypeVariant] {
        &self.variants
    }

    /// Returns whether the type explicitly accepts PHP null.
    pub const fn allows_null(&self) -> bool {
        self.allows_null
    }

    /// Returns whether all variants must match the value.
    pub const fn is_intersection(&self) -> bool {
        matches!(self.kind, EvalParameterTypeKind::Intersection)
    }
}
