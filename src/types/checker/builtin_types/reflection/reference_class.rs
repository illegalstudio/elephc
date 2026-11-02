//! Purpose:
//! Builds the synthetic `ReflectionReference` compatibility class.
//!
//! Called from:
//! - `crate::types::checker::builtin_types::reflection::inject_builtin_reflection()`.
//!
//! Key details:
//! - The public surface matches PHP while runtime hard-reference materialization remains isolated
//!   behind the static factory body.

use super::*;

/// Builds PHP's final `ReflectionReference` shell and its public method contracts.
pub(super) fn builtin_reflection_reference() -> FlattenedClass {
    FlattenedClass {
        name: "ReflectionReference".to_string(),
        span: dummy(),
        extends: None,
        implements: Vec::new(),
        is_abstract: false,
        is_final: true,
        is_readonly_class: false,
        properties: vec![builtin_property(
            "__id",
            Visibility::Private,
            Some(TypeExpr::Str),
            empty_string(),
        )],
        methods: vec![
            builtin_reflection_private_constructor_method(),
            builtin_reflection_reference_from_array_element_method(),
            builtin_reflection_class_string_method("getId", "__id"),
        ],
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    }
}

/// Builds `ReflectionReference::fromArrayElement(array &$array, int|string $key)`.
///
/// Ordinary elements return `null`, which is the common PHP path. The runtime-specific
/// hard-reference probe can replace this conservative body without changing the checker contract.
fn builtin_reflection_reference_from_array_element_method() -> ClassMethod {
    let span = dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: "fromArrayElement".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "array".to_string(),
                Some(array_type()),
                None,
                true,
            ),
            (
                "key".to_string(),
                Some(TypeExpr::Union(vec![TypeExpr::Int, TypeExpr::Str])),
                None,
                false,
            ),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Nullable(Box::new(TypeExpr::Named(
            Name::unqualified("ReflectionReference"),
        )))),
        by_ref_return: false,
        body: vec![Stmt::new(StmtKind::Return(Some(Expr::new(ExprKind::Null, span))), span)],
        span,
        attributes: Vec::new(),
    }
}
