//! Purpose:
//! Shared synthetic-AST builders and DateTime class constants.
//!
//! Called from:
//! - Focused DateTime checker metadata modules.
//!
//! Key details:
//! - All generated nodes use dummy spans and public PHP-visible members.

use super::*;

/// Returns a dummy source span for synthetic AST nodes.
pub(super) fn dummy() -> crate::span::Span {
    crate::span::Span::dummy()
}

/// Builds a public string class constant for the synthetic date/time classes.
pub(super) fn str_class_const(name: &str, value: &str) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        type_expr: None,
        value: Expr::new(ExprKind::StringLiteral(value.to_string()), dummy()),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a public integer class constant for the synthetic date/time classes.
pub(super) fn int_class_const(name: &str, value: i64) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        type_expr: None,
        value: Expr::new(ExprKind::IntLiteral(value), dummy()),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the `DateTimeZone` region/group constants used by `listIdentifiers()`.
/// The per-region bits are powers of two; `ALL` is their OR, `ALL_WITH_BC` adds
/// the backward-compatibility bit (2048), and `PER_COUNTRY` (4096) switches the
/// filter to the country-code argument. Values match PHP exactly.
pub(super) fn datetime_zone_group_constants() -> Vec<ClassConst> {
    vec![
        int_class_const("AFRICA", 1),
        int_class_const("AMERICA", 2),
        int_class_const("ANTARCTICA", 4),
        int_class_const("ARCTIC", 8),
        int_class_const("ASIA", 16),
        int_class_const("ATLANTIC", 32),
        int_class_const("AUSTRALIA", 64),
        int_class_const("EUROPE", 128),
        int_class_const("INDIAN", 256),
        int_class_const("PACIFIC", 512),
        int_class_const("UTC", 1024),
        int_class_const("ALL", 2047),
        int_class_const("ALL_WITH_BC", 4095),
        int_class_const("PER_COUNTRY", 4096),
    ]
}

/// Builds the shared `DateTimeInterface` format constants (`ATOM`, `COOKIE`, the
/// `RFC*` family, `RSS`, `W3C`, ...). PHP exposes them on the interface and, by
/// inheritance, on `DateTime` and `DateTimeImmutable`; the same list is attached
/// to all three synthetic declarations. Values match PHP 8.4 exactly.
pub(super) fn datetime_format_constants() -> Vec<ClassConst> {
    vec![
        str_class_const("ATOM", "Y-m-d\\TH:i:sP"),
        str_class_const("COOKIE", "l, d-M-Y H:i:s T"),
        str_class_const("ISO8601", "Y-m-d\\TH:i:sO"),
        str_class_const("ISO8601_EXPANDED", "X-m-d\\TH:i:sP"),
        str_class_const("RFC822", "D, d M y H:i:s O"),
        str_class_const("RFC850", "l, d-M-y H:i:s T"),
        str_class_const("RFC1036", "D, d M y H:i:s O"),
        str_class_const("RFC1123", "D, d M Y H:i:s O"),
        str_class_const("RFC7231", "D, d M Y H:i:s \\G\\M\\T"),
        str_class_const("RFC2822", "D, d M Y H:i:s O"),
        str_class_const("RFC3339", "Y-m-d\\TH:i:sP"),
        str_class_const("RFC3339_EXTENDED", "Y-m-d\\TH:i:s.vP"),
        str_class_const("RSS", "D, d M Y H:i:s O"),
        str_class_const("W3C", "Y-m-d\\TH:i:sP"),
    ]
}

/// Builds an `$this->property` access expression.
pub(super) fn this_property(property: &str) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::new(ExprKind::This, dummy())),
            property: property.to_string(),
        },
        dummy(),
    )
}

/// Builds a `$this->property = value;` statement.
pub(super) fn assign_this_property(property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(Expr::new(ExprKind::This, dummy())),
            property: property.to_string(),
            value,
        },
        dummy(),
    )
}

/// Builds a `return <expr>;` statement.
pub(super) fn return_expr(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Return(Some(value)), dummy())
}

/// Builds a public instance `ClassMethod` with the given params, return type, and body.
pub(super) fn method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    ClassMethod {
        type_params: Vec::new(),
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params,
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type,
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a public instance `ClassProperty` with a default value.
pub(super) fn property(name: &str, type_expr: TypeExpr, default: Expr) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility: Visibility::Public,
        set_visibility: None,
        type_expr: Some(type_expr),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        is_promoted: false,
        default: Some(default),
        span: dummy(),
        attributes: Vec::new(),
    }
}
