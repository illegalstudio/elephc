//! Purpose:
//! Computes array and iterable element types needed by code generation.
//! Keeps emission-time type decisions separate from instruction lowering.
//!
//! Called from:
//! - `crate::codegen_support::functions::types`
//!
//! Key details:
//! - Results must agree with `crate::types` so local slots and runtime value shapes are selected correctly.

use crate::parser::ast::{Expr, ExprKind};
use crate::types::{merge_array_key_types, PhpType};

/// Computes the wider of two PHP types for codegen emission.
///
/// Returns `b` if `a` is `Mixed` or `Union`, and vice versa, because those types
/// can hold any value. Otherwise selects the "wider" type in the order:
/// `Str` > `Float` > `Array` > `Object` > (int/other). When types are equal,
/// returns that type. Both arguments may be consumed/cloned.
///
/// Used to determine the runtime value shape when merging two candidate types
/// for a variable or expression result.
pub(super) fn wider_of(a: &PhpType, b: &PhpType) -> PhpType {
    if a == b {
        return a.clone();
    }
    if matches!(a, PhpType::Mixed | PhpType::Union(_))
        || matches!(b, PhpType::Mixed | PhpType::Union(_))
    {
        return PhpType::Mixed;
    }
    if *a == PhpType::Str || *b == PhpType::Str {
        return PhpType::Str;
    }
    if *a == PhpType::Float || *b == PhpType::Float {
        return PhpType::Float;
    }
    if matches!(a, PhpType::Array(_)) || matches!(b, PhpType::Array(_)) {
        return a.clone();
    }
    if matches!(a, PhpType::Object(_)) || matches!(b, PhpType::Object(_)) {
        return a.clone();
    }
    a.clone()
}

/// Unwraps the element type from a container type for emission.
///
/// If `ty` is `Iterable`, returns `Mixed` because iterables hold heterogeneous
/// values. Otherwise returns `ty` unchanged. The argument is consumed.
pub(super) fn mixed_container_value_type(ty: PhpType) -> PhpType {
    if matches!(ty, PhpType::Iterable) {
        PhpType::Mixed
    } else {
        ty
    }
}

/// Computes the union type of two array-like types, if a stable union exists.
///
/// Handles `PhpType::Array` with itself, `AssocArray` with itself, and cross-type
/// `Array`/`AssocArray` unions. When key or value types differ, promotes to
/// `Mixed`. Returns `None` for incompatible pairs (e.g., `Object` + `Array`).
/// Used to determine the result type of array concatenation and spread operators.
pub(super) fn array_union_type(a: &PhpType, b: &PhpType) -> Option<PhpType> {
    match (a, b) {
        (PhpType::Array(left), PhpType::Array(right)) if left == right => {
            Some(PhpType::Array(left.clone()))
        }
        (
            PhpType::AssocArray {
                key: left_key,
                value: left_value,
            },
            PhpType::AssocArray {
                key: right_key,
                value: right_value,
            },
        ) => {
            let key = if left_key == right_key {
                left_key.clone()
            } else {
                Box::new(PhpType::Mixed)
            };
            let value = if left_value == right_value {
                left_value.clone()
            } else {
                Box::new(PhpType::Mixed)
            };
            Some(PhpType::AssocArray { key, value })
        }
        (PhpType::Array(left_value), PhpType::AssocArray { key, value }) => {
            Some(PhpType::AssocArray {
                key: Box::new(merge_array_key_types(PhpType::Int, *key.clone())),
                value: Box::new(array_union_value_type(left_value, value)),
            })
        }
        (PhpType::AssocArray { key, value }, PhpType::Array(right_value)) => {
            Some(PhpType::AssocArray {
                key: Box::new(merge_array_key_types(*key.clone(), PhpType::Int)),
                value: Box::new(array_union_value_type(value, right_value)),
            })
        }
        _ => None,
    }
}

/// Computes the union of two array element types.
///
/// Returns `left` if both types are equal, prefers the non-`Never` type when one
/// is `Never`, and returns `Mixed` otherwise. Used only within `array_union_type`
/// when merging `PhpType::Array` with `PhpType::AssocArray` element types.
fn array_union_value_type(left: &PhpType, right: &PhpType) -> PhpType {
    if left == right {
        left.clone()
    } else if matches!(left, PhpType::Never) {
        right.clone()
    } else if matches!(right, PhpType::Never) {
        left.clone()
    } else {
        PhpType::Mixed
    }
}

/// Returns the key type for an array-like PHP type.
///
/// `PhpType::Array` implies integer keys; `PhpType::AssocArray` returns its stored
/// key type; all other types default to `Int`. The argument is borrowed only.
pub(super) fn array_like_key_type(ty: &PhpType) -> PhpType {
    match ty {
        PhpType::Array(_) => PhpType::Int,
        PhpType::AssocArray { key, .. } => *key.clone(),
        _ => PhpType::Int,
    }
}

/// Returns the value type for an array-like PHP type.
///
/// `PhpType::Array` returns its element type; `PhpType::AssocArray` returns its
/// stored value type; all other types default to `Int`. The argument is borrowed only.
pub(super) fn array_like_value_type(ty: &PhpType) -> PhpType {
    match ty {
        PhpType::Array(value) => *value.clone(),
        PhpType::AssocArray { value, .. } => *value.clone(),
        _ => PhpType::Int,
    }
}

/// Returns the element type of a typed array, or a fallback for non-array types.
///
/// For `PhpType::Array(value)` returns the element type. For all other types,
/// returns the provided `fallback`. The argument is borrowed only; `fallback` is consumed.
pub(super) fn indexed_array_value_type(ty: &PhpType, fallback: PhpType) -> PhpType {
    match ty {
        PhpType::Array(value) => *value.clone(),
        _ => fallback,
    }
}

/// Checks whether an expression is an empty array literal.
///
/// Returns `true` only for `ExprKind::ArrayLiteral` with no elements. Used to
/// identify zero-length typed arrays that may require special emission handling.
/// The argument is borrowed only.
pub(super) fn is_empty_indexed_array_literal(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::ArrayLiteral(elems) if elems.is_empty())
}
