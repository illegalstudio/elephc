//! Purpose:
//! Separates public Zend constant types from checked native numeric contracts.
//!
//! Called from:
//! - Constant registration, reference resolution and hosted schema materialization.
//!
//! Key details:
//! - Numeric widening uses the shared PhpType contract, including nominal union members.
//! - Native implementation types do not expand the admitted public metadata surface.

use crate::{parser::ast::TypeExpr, types::PhpType};

/// Resolves the supported constant type atoms through the shared PHP value contract.
pub(super) fn declared_type(ty: &TypeExpr) -> Result<PhpType, String> {
    Ok(match ty {
        TypeExpr::Int => PhpType::Int, TypeExpr::Float => PhpType::Float, TypeExpr::Bool => PhpType::Bool,
        TypeExpr::False => PhpType::False, TypeExpr::Str => PhpType::Str,
        TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("mixed") => PhpType::Mixed,
        TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("null") => PhpType::Void,
        TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("array") => PhpType::php_array(),
        TypeExpr::Nullable(inner) => PhpType::Union(vec![declared_type(inner)?, PhpType::Void]),
        TypeExpr::Union(members) => PhpType::Union(members.iter().map(declared_type).collect::<Result<Vec<_>, _>>()?),
        _ => return Err("unsupported PHP extension class constant type".into()),
    })
}

/// Maps checked type categories for numeric widening without admitting them to Zend metadata.
pub(super) fn coercion_type(ty: &TypeExpr) -> PhpType {
    let ty = match ty {
        TypeExpr::Nullable(inner) => PhpType::Union(vec![coercion_type(inner), PhpType::Void]),
        TypeExpr::Union(members) => PhpType::Union(members.iter().map(coercion_type).collect()),
        TypeExpr::Intersection(members) => members.first().map(coercion_type).unwrap_or(PhpType::Mixed),
        TypeExpr::Named(name) => match name.as_str().to_ascii_lowercase().as_str() {
            "int" => PhpType::Int, "float" => PhpType::Float, "bool" => PhpType::Bool,
            "false" => PhpType::False, "string" => PhpType::Str, "mixed" => PhpType::Mixed,
            "null" | "void" => PhpType::Void, "array" => PhpType::php_array(),
            "callable" | "closure" => PhpType::Callable,
            _ => PhpType::Object(name.as_str().to_string()),
        },
        TypeExpr::Array(inner) => PhpType::Array(Box::new(coercion_type(inner))),
        TypeExpr::AssocArray { key, value } => PhpType::AssocArray {
            key: Box::new(coercion_type(key)), value: Box::new(coercion_type(value)),
        },
        TypeExpr::Ptr(name) => PhpType::Pointer(name.as_ref().map(|name| name.as_str().to_string())),
        TypeExpr::Buffer(inner) => PhpType::Buffer(Box::new(coercion_type(inner))),
        TypeExpr::CallableSig { .. } => PhpType::Callable,
        TypeExpr::GenericClass { name, .. } => PhpType::Object(name.as_str().to_string()),
        TypeExpr::Iterable => PhpType::Iterable, TypeExpr::Void => PhpType::Void,
        TypeExpr::Never => PhpType::Never,
        _ => declared_type(ty).unwrap_or(PhpType::Mixed),
    };
    match ty {
        PhpType::Union(members) => PhpType::Union(members.into_iter().flat_map(|member| match member {
            PhpType::Union(members) => members, member => vec![member],
        }).collect()),
        ty => ty,
    }
}
