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

pub(super) fn propagate_property(property: ClassProperty) -> ClassProperty {
    ClassProperty {
        name: property.name,
        visibility: property.visibility,
        type_expr: property.type_expr,
        readonly: property.readonly,
        is_final: property.is_final,
        is_static: property.is_static,
        by_ref: property.by_ref,
        default: property
            .default
            .map(|expr| propagate_expr(expr, &HashMap::new())),
        span: property.span,
        attributes: property.attributes,
    }
}

pub(super) fn propagate_method(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        params: propagate_params(method.params),
        body: propagate_block(method.body, HashMap::new()).0,
        ..method
    }
}

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
