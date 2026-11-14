//! Purpose:
//! Shares parsed constant contracts and already-validated scalar literal binding.
//!
//! Called from:
//! - Class, interface and enum schema metadata construction.
//! - PHP extension constant resolution and native metadata materialization.
//!
//! Key details:
//! - Original source expressions stay available to strict validation before metadata is bound.
//! - Only integer-to-float literal widening changes a value; unresolved expressions stay intact.

use crate::parser::ast::{Expr, ExprKind, TypeExpr};
use crate::types::PhpType;

/// Projects checked annotation categories without imposing the PHP extension's public type subset.
pub(crate) fn declared_type(ty: &TypeExpr) -> PhpType {
    let ty = match ty {
        TypeExpr::Nullable(inner) => PhpType::Union(vec![declared_type(inner), PhpType::Void]),
        TypeExpr::Union(members) => PhpType::Union(members.iter().map(declared_type).collect()),
        TypeExpr::Intersection(members) => members.first().map(declared_type).unwrap_or(PhpType::Mixed),
        TypeExpr::Named(name) => match name.as_str().to_ascii_lowercase().as_str() {
            "int" => PhpType::Int, "float" => PhpType::Float, "bool" => PhpType::Bool,
            "false" => PhpType::False, "string" => PhpType::Str, "mixed" => PhpType::Mixed,
            "null" | "void" => PhpType::Void, "array" => PhpType::php_array(),
            "callable" | "closure" => PhpType::Callable,
            _ => PhpType::Object(name.as_str().to_string()),
        },
        TypeExpr::Array(inner) => PhpType::Array(Box::new(declared_type(inner))),
        TypeExpr::AssocArray { key, value } => PhpType::AssocArray {
            key: Box::new(declared_type(key)), value: Box::new(declared_type(value)),
        },
        TypeExpr::Ptr(name) => PhpType::Pointer(name.as_ref().map(|name| name.as_str().to_string())),
        TypeExpr::Buffer(inner) => PhpType::Buffer(Box::new(declared_type(inner))),
        TypeExpr::CallableSig { .. } => PhpType::Callable,
        TypeExpr::GenericClass { name, .. } => PhpType::Object(name.as_str().to_string()),
        TypeExpr::Iterable => PhpType::Iterable, TypeExpr::Void => PhpType::Void,
        TypeExpr::Never => PhpType::Never,
        _ => crate::semantics::conversions::scalar_hint(ty).unwrap_or(PhpType::Mixed),
    };
    match ty {
        PhpType::Union(members) => PhpType::Union(members.into_iter().flat_map(|member| match member {
            PhpType::Union(members) => members, member => vec![member],
        }).collect()),
        ty => ty,
    }
}

/// Materializes a validated literal under its declaration without treating a read as a new conversion.
pub(crate) fn bind_literal(mut value: Expr, declared: &PhpType) -> Expr {
    if declared.coerces_integer_to_float() {
        if let ExprKind::IntLiteral(integer) = value.kind { value.kind = ExprKind::FloatLiteral(integer as f64); }
    }
    value
}

/// Binds a schema copy while leaving untyped and unresolved source expressions unchanged.
pub(crate) fn bind_declared_literal(value: Expr, annotation: Option<&TypeExpr>) -> Expr {
    match annotation {
        Some(annotation) => bind_literal(value, &declared_type(annotation)),
        None => value,
    }
}
