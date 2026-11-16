//! Purpose:
//! Materializes generic PHP value annotations without erasing native storage widths.
//!
//! Called from:
//! - Generic declaration splicing and checker signature materialization.
//!
//! Key details:
//! - Widths bind PHP scalar carriers only in value positions. Buffer elements and class
//!   type arguments retain their original bindings, including their storage identity.

use crate::parser::ast::TypeExpr;
use crate::types::width_type_from_type_expr;

use super::Bindings;

/// Substitutes a PHP value annotation, mapping a bound native width to `int` or `float`.
/// Literal width annotations remain unchanged so the checker still rejects them as types.
pub(crate) fn substitute_value_type(ty: &TypeExpr, bindings: &Bindings) -> TypeExpr {
    rewrite_value_type(ty, bindings, false)
}

/// Converts a resolved generic argument to its PHP value shape without changing storage arguments.
pub(crate) fn value_carrier_type(ty: &TypeExpr) -> TypeExpr {
    rewrite_value_type(ty, &[], true)
}

/// Walks PHP value structure, optionally normalizing width atoms inside a concrete binding.
fn rewrite_value_type(
    ty: &TypeExpr, bindings: &[(String, TypeExpr)], map_widths: bool,
) -> TypeExpr {
    match ty {
        TypeExpr::Named(name) => bindings
            .iter()
            .find(|(param, _)| param == name.as_str())
            .map(|(_, bound)| value_carrier_type(bound))
            .unwrap_or_else(|| {
                if map_widths {
                    if let Some(storage) = width_type_from_type_expr(ty) {
                        return super::type_expr_for_php_type(&storage.storage_value_type())
                            .expect("a storage width has a PHP scalar carrier");
                    }
                }
                ty.clone()
            }),
        TypeExpr::Nullable(inner) => {
            TypeExpr::Nullable(Box::new(rewrite_value_type(inner, bindings, map_widths)))
        }
        TypeExpr::Array(inner) => TypeExpr::Array(Box::new(rewrite_value_type(inner, bindings, map_widths))),
        TypeExpr::AssocArray { key, value } => TypeExpr::AssocArray {
            key: Box::new(rewrite_value_type(key, bindings, map_widths)),
            value: Box::new(rewrite_value_type(value, bindings, map_widths)),
        },
        TypeExpr::Union(members) => TypeExpr::Union(
            members.iter().map(|member| rewrite_value_type(member, bindings, map_widths)).collect(),
        ),
        TypeExpr::Intersection(members) => TypeExpr::Intersection(
            members.iter().map(|member| rewrite_value_type(member, bindings, map_widths)).collect(),
        ),
        TypeExpr::CallableSig { params, ret } => TypeExpr::CallableSig {
            params: params.iter().map(|param| rewrite_value_type(param, bindings, map_widths)).collect(),
            ret: Box::new(rewrite_value_type(ret, bindings, map_widths)),
        },
        TypeExpr::Buffer(_) | TypeExpr::GenericClass { .. } => ty.substitute_type_params(bindings),
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable
        | TypeExpr::Ptr(_) => ty.clone(),
    }
}
