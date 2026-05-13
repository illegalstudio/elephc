//! Purpose:
//! Computes array and iterable element types needed by code generation.
//! Keeps emission-time type decisions separate from instruction lowering.
//!
//! Called from:
//! - `crate::codegen::functions::types`
//!
//! Key details:
//! - Results must agree with `crate::types` so local slots and runtime value shapes are selected correctly.

use crate::parser::ast::{Expr, ExprKind};
use crate::types::{merge_array_key_types, PhpType};

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

pub(super) fn mixed_container_value_type(ty: PhpType) -> PhpType {
    if matches!(ty, PhpType::Iterable) {
        PhpType::Mixed
    } else {
        ty
    }
}

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

pub(super) fn array_like_key_type(ty: &PhpType) -> PhpType {
    match ty {
        PhpType::Array(_) => PhpType::Int,
        PhpType::AssocArray { key, .. } => *key.clone(),
        _ => PhpType::Int,
    }
}

pub(super) fn array_like_value_type(ty: &PhpType) -> PhpType {
    match ty {
        PhpType::Array(value) => *value.clone(),
        PhpType::AssocArray { value, .. } => *value.clone(),
        _ => PhpType::Int,
    }
}

pub(super) fn indexed_array_value_type(ty: &PhpType, fallback: PhpType) -> PhpType {
    match ty {
        PhpType::Array(value) => *value.clone(),
        _ => fallback,
    }
}

pub(super) fn is_empty_indexed_array_literal(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::ArrayLiteral(elems) if elems.is_empty())
}
