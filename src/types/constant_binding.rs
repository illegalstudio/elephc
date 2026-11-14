//! Purpose:
//! Shares parsed constant contracts and prepared declaration producer binding.
//!
//! Called from:
//! - Class, interface and enum schema metadata construction.
//! - PHP extension constant resolution and native metadata materialization.
//!
//! Key details:
//! - Original source expressions stay available to strict validation before metadata is bound.
//! - Prepared copies retain source expressions and record integer widening after evaluation.

use crate::parser::ast::{Expr, ExprKind, TypeExpr};
use crate::types::PhpType;

/// Preserves non-integer alternatives when a prepared producer widens its integer result.
pub(crate) fn bound_result_type(ty: PhpType, binding: Option<crate::parser::ast::ProducerBinding>) -> PhpType {
    if binding != Some(crate::parser::ast::ProducerBinding::IntegerToFloat) { return ty; }
    match ty {
        PhpType::Int => PhpType::Float,
        PhpType::Union(members) => crate::types::checker::normalize_union_members(
            members.into_iter().map(|member| bound_result_type(member, binding)).collect()),
        other => other,
    }
}

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

/// Prepares a validated declaration copy without turning its later read into a new conversion.
pub(crate) fn bind_value(value: Expr, declared: &PhpType) -> Expr {
    let binding = if declared.coerces_integer_to_float() {
        crate::parser::ast::ProducerBinding::IntegerToFloat
    } else { crate::parser::ast::ProducerBinding::Identity };
    apply_binding(value, binding)
}

/// Materializes an integer literal immediately and records binding for unresolved producer results.
pub(crate) fn apply_binding(mut value: Expr, binding: crate::parser::ast::ProducerBinding) -> Expr {
    value.producer_binding = Some(binding);
    if binding == crate::parser::ast::ProducerBinding::IntegerToFloat {
        if let ExprKind::IntLiteral(integer) = value.kind { value.kind = ExprKind::FloatLiteral(integer as f64); }
    }
    value
}

/// Prepares a schema copy while preserving its original source expression and physical policy.
pub(crate) fn bind_declared_value(value: Expr, annotation: Option<&TypeExpr>) -> Expr {
    match annotation {
        Some(annotation) => bind_value(value, &declared_type(annotation)),
        None => bind_value(value, &PhpType::Mixed),
    }
}

/// Prepares existing property defaults after original declaration validation has completed.
pub(crate) fn bind_property_defaults(info: &mut crate::types::ClassInfo) {
    for (index, ((_, ty), default)) in info.properties.iter().zip(&mut info.defaults).enumerate() {
        if let Some(value) = default {
            let destination = if info.property_declared_slots.get(index).copied().unwrap_or(false) {
                ty
            } else { &PhpType::Mixed };
            *value = bind_value(value.clone(), destination);
        }
    }
    for ((name, ty), default) in info.static_properties.iter().zip(&mut info.static_defaults) {
        if let Some(value) = default {
            let destination = if info.declared_static_properties.contains(name) { ty }
                else { &PhpType::Mixed };
            *value = bind_value(value.clone(), destination);
        }
    }
}
