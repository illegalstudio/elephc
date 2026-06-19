//! Purpose:
//! Computes AST type-expression conversion for codegen needed by code generation.
//! Keeps emission-time type decisions separate from instruction lowering.
//!
//! Called from:
//! - `crate::codegen::functions::types`
//!
//! Key details:
//! - Results must agree with `crate::types` so local slots and runtime value shapes are selected correctly.

use crate::codegen::context::Context;
use crate::parser::ast::TypeExpr;
use crate::types::PhpType;

use super::union::merge_union_members;

/// Recursively resolves a `Buffer` element type to its inner `PhpType`.
/// Used during codegen to determine the element type stored in a buffer.
/// Falls back to `PhpType::Int` for unknown types within buffers.
pub(super) fn resolve_buffer_element_type(type_expr: &TypeExpr, ctx: &Context) -> PhpType {
    match type_expr {
        TypeExpr::Int => PhpType::Int,
        TypeExpr::Float => PhpType::Float,
        TypeExpr::Bool => PhpType::Bool,
        TypeExpr::Ptr(target) => {
            PhpType::Pointer(target.as_ref().map(|name| name.as_str().to_string()))
        }
        TypeExpr::Named(name) => {
            if ctx.packed_classes.contains_key(name.as_str()) {
                PhpType::Packed(name.as_str().to_string())
            } else {
                PhpType::Int
            }
        }
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Never => PhpType::Never,
        TypeExpr::Buffer(inner) => {
            PhpType::Buffer(Box::new(resolve_buffer_element_type(inner, ctx)))
        }
        TypeExpr::Array(_)
        | TypeExpr::Iterable
        | TypeExpr::Nullable(_)
        | TypeExpr::Union(_)
        | TypeExpr::Intersection(_) => PhpType::Int,
    }
}

/// Converts a type expression to the `PhpType` used for codegen slot allocation and
/// runtime value representation. Does not resolve nullable/union wrappers — those
/// are handled separately by `codegen_static_type`. For named types, resolves class
/// names to `PhpType::Object` or `PhpType::Packed` when the class is known;
/// unknown names fall back to `PhpType::Int`.
pub(crate) fn codegen_declared_type(type_expr: &TypeExpr, ctx: &Context) -> PhpType {
    match type_expr {
        TypeExpr::Int => PhpType::Int,
        TypeExpr::Float => PhpType::Float,
        TypeExpr::Bool => PhpType::Bool,
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Never => PhpType::Never,
        TypeExpr::Iterable => PhpType::Iterable,
        TypeExpr::Ptr(target) => {
            PhpType::Pointer(target.as_ref().map(|name| name.as_str().to_string()))
        }
        TypeExpr::Buffer(inner) => {
            PhpType::Buffer(Box::new(resolve_buffer_element_type(inner, ctx)))
        }
        TypeExpr::Array(inner) => PhpType::Array(Box::new(codegen_static_type(inner, ctx))),
        TypeExpr::Named(name) => match name.as_str() {
            "string" => PhpType::Str,
            "mixed" => PhpType::Mixed,
            "callable" => PhpType::Callable,
            "object" => PhpType::Object(String::new()),
            "void" => PhpType::Void,
            "array" => PhpType::Array(Box::new(PhpType::Mixed)),
            _ if ctx.packed_classes.contains_key(name.as_str()) => {
                PhpType::Packed(name.as_str().to_string())
            }
            _ if ctx.classes.contains_key(name.as_str())
                || ctx.interfaces.contains_key(name.as_str())
                || ctx.extern_classes.contains_key(name.as_str()) =>
            {
                PhpType::Object(name.as_str().to_string())
            }
            _ => PhpType::Int,
        },
        // An intersection value is an object pointer; type it as its first member.
        TypeExpr::Intersection(members) => members
            .first()
            .map(|member| codegen_declared_type(member, ctx))
            .unwrap_or(PhpType::Int),
        TypeExpr::Nullable(_) | TypeExpr::Union(_) => PhpType::Mixed,
    }
}

/// Resolves the static `PhpType` for a type expression, including nullable and union
/// types. Nullable types are flattened into a union of the inner type and `void`.
/// Union types are merged using `merge_union_members`. Non-union/non-nullable types
/// delegate to `codegen_declared_type`.
pub(crate) fn codegen_static_type(type_expr: &TypeExpr, ctx: &Context) -> PhpType {
    match type_expr {
        TypeExpr::Nullable(inner) => {
            merge_union_members(vec![codegen_static_type(inner, ctx), PhpType::Void])
        }
        TypeExpr::Union(members) => merge_union_members(
            members
                .iter()
                .map(|member| codegen_static_type(member, ctx))
                .collect(),
        ),
        _ => codegen_declared_type(type_expr, ctx),
    }
}
