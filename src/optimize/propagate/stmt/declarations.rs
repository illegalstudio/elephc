//! Purpose:
//! Propagates constants through statement declarations cases.
//! Maintains scalar environments while preserving declarations and control-flow side effects.
//!
//! Called from:
//! - `crate::optimize::propagate::stmt`
//!
//! Key details:
//! - Statement propagation must invalidate aliases and writes before substituting values across observable boundaries.

use super::*;

/// Propagates constant values into default parameter expressions.
///
/// Maps over each parameter, recursively propagating constants in default values
/// using an empty scalar environment (no prior bindings are in scope for params).
/// Reference parameters and type annotations are preserved unchanged.
pub(crate) fn propagate_params(
    params: Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>,
) -> Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)> {
    params
        .into_iter()
        .map(|(name, type_expr, default, is_ref)| {
            (
                name,
                type_expr,
                default.map(|expr| propagate_expr(expr, &HashMap::new())),
                is_ref,
            )
        })
        .collect()
}

/// Propagates constant values into a class property's default value expression.
///
/// Recursively propagates constants in the property's default value using an empty
/// scalar environment. All other fields (name, visibility, type, hooks, etc.) are
/// copied unchanged.
pub(super) fn propagate_property(property: ClassProperty) -> ClassProperty {
    ClassProperty {
        name: property.name,
        visibility: property.visibility,
        type_expr: property.type_expr,
        hooks: property.hooks,
        readonly: property.readonly,
        is_final: property.is_final,
        is_static: property.is_static,
        is_abstract: property.is_abstract,
        by_ref: property.by_ref,
        default: property
            .default
            .map(|expr| propagate_expr(expr, &HashMap::new())),
        span: property.span,
        attributes: property.attributes,
    }
}

/// Propagates constant values through a class method's parameters and body.
///
/// Recursively propagates constants in the method's parameter defaults and body block
/// using a fresh empty scalar environment (no enclosing scope bindings). Other method
/// metadata (name, visibility, attributes, return type, etc.) is copied unchanged.
pub(super) fn propagate_method(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        params: propagate_params(method.params),
        body: propagate_block(method.body, HashMap::new()).0,
        ..method
    }
}

/// Propagates constant values into an enum case's value expression.
///
/// Recursively propagates constants in the enum case value using an empty scalar
/// environment. Other case metadata (name, span, attributes) is copied unchanged.
pub(super) fn propagate_enum_case(case: EnumCaseDecl) -> EnumCaseDecl {
    EnumCaseDecl {
        name: case.name,
        value: case
            .value
            .map(|expr| propagate_expr(expr, &HashMap::new())),
        span: case.span,
        attributes: case.attributes,
    }
}
