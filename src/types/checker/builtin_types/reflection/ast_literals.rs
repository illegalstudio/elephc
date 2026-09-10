//! Purpose:
//! Builds synthetic Reflection properties, literals, and type expressions.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - These helpers centralize dummy AST nodes and collection result shapes.

use super::*;

/// Builds a `ClassProperty` for a built-in reflection type with the given name,
/// visibility, optional type expression, and optional default value.
pub(super) fn builtin_property(
    name: &str,
    visibility: Visibility,
    type_expr: Option<TypeExpr>,
    default: Option<Expr>,
) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility,
        set_visibility: None,
        type_expr,
        hooks: crate::parser::ast::PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        is_promoted: false,
        default,
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Returns a `StringLiteral` expression with an empty string value.
pub(super) fn empty_string() -> Option<Expr> {
    Some(Expr::new(
        ExprKind::StringLiteral(String::new()),
        crate::span::Span::dummy(),
    ))
}

/// Returns an `ArrayLiteral` expression with no elements.
pub(super) fn empty_array() -> Option<Expr> {
    Some(Expr::new(
        ExprKind::ArrayLiteral(Vec::new()),
        crate::span::Span::dummy(),
    ))
}

/// Returns a `BoolLiteral(false)` expression.
pub(super) fn false_bool() -> Option<Expr> {
    Some(Expr::new(
        ExprKind::BoolLiteral(false),
        crate::span::Span::dummy(),
    ))
}

/// Returns a `BoolLiteral(true)` expression.
pub(super) fn true_bool() -> Option<Expr> {
    Some(Expr::new(
        ExprKind::BoolLiteral(true),
        crate::span::Span::dummy(),
    ))
}

/// Returns an `IntLiteral` expression with the given value.
pub(super) fn int_lit(value: i64) -> Option<Expr> {
    Some(Expr::new(
        ExprKind::IntLiteral(value),
        crate::span::Span::dummy(),
    ))
}

/// Returns a `Null` literal expression.
pub(super) fn null_lit() -> Option<Expr> {
    Some(Expr::new(ExprKind::Null, crate::span::Span::dummy()))
}

/// Returns a `BoolLiteral` expression with the given value.
pub(super) fn bool_lit(value: bool) -> Option<Expr> {
    Some(Expr::new(
        ExprKind::BoolLiteral(value),
        crate::span::Span::dummy(),
    ))
}

/// Returns a `null` expression for nullable synthetic property defaults.
pub(super) fn null_expr() -> Option<Expr> {
    Some(Expr::new(ExprKind::Null, crate::span::Span::dummy()))
}

/// Returns a `TypeExpr` for the unqualified name `array`.
pub(super) fn array_type() -> TypeExpr {
    TypeExpr::Named(crate::names::Name::unqualified("array"))
}

/// Returns a `TypeExpr` for an indexed array of strings.
pub(super) fn string_array_type() -> TypeExpr {
    TypeExpr::Array(Box::new(TypeExpr::Str))
}

/// Returns a `TypeExpr` for an indexed array of objects with the given class name.
pub(super) fn object_array_type(class_name: &str) -> TypeExpr {
    TypeExpr::Array(Box::new(TypeExpr::Named(Name::unqualified(class_name))))
}

/// Returns `array<string, ReflectionClass>` for name-keyed reflection maps.
pub(super) fn reflection_class_object_map_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Object("ReflectionClass".to_string())),
    }
}

/// Returns `array<string, mixed>` for `ReflectionExtension::getClasses()`.
///
/// The collection deliberately admits both `ReflectionClass` and `ReflectionEnum`
/// instances, which are separate flattened synthetic checker classes.
pub(super) fn reflection_extension_class_map_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    }
}

/// Returns `array<string, ReflectionFunction>` for extension function collections.
pub(super) fn reflection_extension_function_map_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Object("ReflectionFunction".to_string())),
    }
}

/// Returns `array<string, string>` for trait-alias reflection maps.
pub(super) fn reflection_string_map_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Str),
    }
}

/// Returns `array<string, mixed>` for static-property value reflection maps.
pub(super) fn reflection_static_properties_map_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    }
}

/// Returns `array<int|string, mixed>` for ReflectionAttribute argument maps.
pub(super) fn reflection_attribute_args_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    }
}

/// Returns `array<string, ReflectionMethod>` for property-hook reflection maps.
pub(super) fn reflection_property_hook_map_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Object("ReflectionMethod".to_string())),
    }
}

/// Returns a nullable object type expression for one synthetic reflection class.
pub(super) fn nullable_object_type(class_name: &str) -> TypeExpr {
    TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(class_name))))
}

/// Returns a `TypeExpr` for PHP's generic `object` type.
pub(super) fn object_type() -> TypeExpr {
    TypeExpr::Named(Name::unqualified("object"))
}

/// Returns a `TypeExpr` for the unqualified name `mixed`.
pub(super) fn mixed_type() -> TypeExpr {
    TypeExpr::Named(crate::names::Name::unqualified("mixed"))
}

/// Returns a `TypeExpr` for PHP's builtin boolean type.
pub(super) fn bool_type() -> TypeExpr {
    TypeExpr::Bool
}

/// Returns a `TypeExpr` for Reflection APIs whose PHP return is `string|false`.
pub(super) fn string_or_bool_type() -> TypeExpr {
    TypeExpr::Union(vec![TypeExpr::Str, TypeExpr::Bool])
}
