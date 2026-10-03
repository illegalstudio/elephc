//! Purpose:
//! Classifies PHP's `(array)` cast by its source type, as one decision shared by the type
//! checker and the EIR lowering.
//!
//! Called from:
//! - `crate::types::checker::inference::expr::basic` (the cast's static type)
//! - `crate::ir_lower::expr::ternary_cast` (the conversion to emit)
//!
//! Key details:
//! - The checker's type and the lowered value's storage MUST agree: a cast typed as a packed
//!   array of strings but lowered as a boxed cell is read back as the wrong layout. Deriving
//!   both from one classification is what keeps them in step.
//! - PHP returns an array unchanged, turns `null` into `[]`, wraps a scalar as `[0 => value]`,
//!   and projects an object to its property map. A runtime-typed source is dispatched by tag.

use super::PhpType;

/// How an `(array)` cast converts a value of a given static type.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ArrayCast {
    /// The source is already an array; PHP returns it unchanged.
    Identity,
    /// The source is an object; the result is its visibility-mangled property map.
    ObjectProperties,
    /// The source is a scalar; the result is a one-element packed array of this element type.
    WrapScalar(PhpType),
    /// The source is `null`; the result is an empty array.
    Empty,
    /// The source's kind is only known at runtime; the value is boxed (when it is not already)
    /// and dispatched on its runtime tag.
    Dynamic,
}

impl ArrayCast {
    /// Classifies an `(array)` cast of a value whose static type is `source`.
    pub(crate) fn for_source(source: &PhpType) -> Self {
        if source.is_php_array() {
            return Self::Identity;
        }
        // A resource's codegen representation is its integer id, but PHP wraps the resource
        // itself, so it must keep its runtime tag through the boxed path.
        if matches!(source, PhpType::Resource(_)) {
            return Self::Dynamic;
        }
        match source.codegen_repr() {
            PhpType::Array(_) | PhpType::AssocArray { .. } => Self::Identity,
            PhpType::Object(_) => Self::ObjectProperties,
            scalar @ (PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str) => {
                Self::WrapScalar(scalar)
            }
            PhpType::Void => Self::Empty,
            _ => Self::Dynamic,
        }
    }

    /// Returns the static PHP type of the cast's result for a `source` of this classification.
    pub(crate) fn result_type(&self, source: &PhpType) -> PhpType {
        match self {
            Self::Identity => source.clone(),
            Self::ObjectProperties => PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            },
            Self::WrapScalar(element) => PhpType::Array(Box::new(element.clone())),
            Self::Empty => PhpType::Array(Box::new(PhpType::Never)),
            Self::Dynamic => PhpType::php_array(),
        }
    }
}

/// Returns the static PHP type of `(array)` applied to a value of type `source`.
pub(crate) fn array_cast_result_type(source: &PhpType) -> PhpType {
    ArrayCast::for_source(source).result_type(source)
}
