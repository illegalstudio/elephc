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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct EvalFunction {
    name: String,
    source_location: Option<EvalSourceLocation>,
    attributes: Vec<EvalAttribute>,
    params: Vec<String>,
    parameter_attributes: Vec<Vec<EvalAttribute>>,
    parameter_types: Vec<Option<EvalParameterType>>,
    parameter_defaults: Vec<Option<EvalExpr>>,
    parameter_is_by_ref: Vec<bool>,
    parameter_is_variadic: Vec<bool>,
    return_type: Option<EvalParameterType>,
    /// Whether the function was declared `function &name()`, PHP's return-by-reference form.
    returns_by_ref: bool,
    /// Whether the FILE that declared this was under `declare(strict_types=1)`.
    ///
    /// php scopes strict typing to the file containing the code doing the coercing, so a `return`
    /// obeys the file the function was DECLARED in while an argument obeys the file the CALL was
    /// written in. Stamping the declaration is what lets the interpreter restore the right mode
    /// while a body runs. Deliberately absent from `PartialEq`, like `source_location`: two
    /// declarations of the same shape are the same declaration whatever file mode they were read
    /// under, and every parser-shape expectation compares shapes.
    strict_types: bool,
    body: Vec<EvalStmt>,
}

impl PartialEq for EvalFunction {
    /// Compares function metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.attributes == other.attributes
            && self.params == other.params
            && self.parameter_attributes == other.parameter_attributes
            && self.parameter_types == other.parameter_types
            && self.parameter_defaults == other.parameter_defaults
            && self.parameter_is_by_ref == other.parameter_is_by_ref
            && self.parameter_is_variadic == other.parameter_is_variadic
            && self.return_type == other.return_type
            && self.returns_by_ref == other.returns_by_ref
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
        Self {
            name: name.into(),
            source_location: None,
            attributes: Vec::new(),
            params,
            parameter_attributes,
            parameter_types,
            parameter_defaults,
            parameter_is_by_ref,
            parameter_is_variadic,
            return_type: None,
            returns_by_ref: false,
            strict_types: false,
            body,
        }
    }

    /// Returns a copy of this function stamped with its declaring file's strict-types mode.
    pub const fn with_strict_types(mut self, strict_types: bool) -> Self {
        self.strict_types = strict_types;
        self
    }

    /// Returns whether the file that declared this function was under `strict_types=1`.
    pub const fn strict_types(&self) -> bool {
        self.strict_types
    }

    /// Returns a copy of this function with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
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

    /// Returns a copy of this function marked as returning by reference.
    ///
    /// PHP's `function &name()`. It is recorded rather than dropped because an implementation
    /// may ADD the `&` its interface does not require but may never REMOVE one it does:
    /// `php -n` 8.5.6 refuses that with `Declaration of Bad::get() must be compatible with
    /// & Ref::get()`.
    pub const fn with_returns_by_ref(mut self, returns_by_ref: bool) -> Self {
        self.returns_by_ref = returns_by_ref;
        self
    }

    /// Returns whether this function was declared to return by reference.
    pub const fn returns_by_ref(&self) -> bool {
        self.returns_by_ref
    }

    /// Returns the original source spelling of this eval-declared function name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum EvalParameterTypeVariant {
    Generic(elephc_generics::Type),
    Array,
    Bool,
    Callable,
    Class(String),
    /// PHP 8.2's standalone `false` type. Kept distinct from `Bool` because it accepts only the
    /// one value: `function f(): false` returning `true` is a TypeError in PHP. Without it both
    /// `false` and `true` parsed as a CLASS named "false", so Symfony's
    /// `ControllerResolver::getController(): callable|false` threw on its ordinary
    /// "no `_controller` attribute" path — the 404 route — instead of returning.
    False,
    Float,
    Int,
    Iterable,
    Mixed,
    Never,
    Object,
    String,
    /// PHP 8.2's standalone `true` type, the mirror of [`EvalParameterTypeVariant::False`].
    True,
    Void,
}

/// How multiple eval parameter type atoms combine.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalParameterTypeKind {
    Union,
    Intersection,
}

/// Type metadata retained for one eval method parameter.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
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
        Self::intersection_allowing_null(variants, false)
    }

    /// Creates one eval parameter type from intersection variants that may also accept null.
    ///
    /// PHP 8.2's disjunctive normal form types write a parenthesized intersection inside a union,
    /// and `(A&B)|null` is the shape the standard library and Symfony use: every atom must match,
    /// or the value is null.
    pub fn intersection_allowing_null(
        variants: Vec<EvalParameterTypeVariant>,
        allows_null: bool,
    ) -> Self {
        Self {
            variants,
            allows_null,
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
