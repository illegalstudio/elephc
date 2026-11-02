//! Purpose:
//! Builds the internal static `__elephc_date_create()` factory behind the procedural
//! `date_create()` / `date_create_immutable()` aliases.
//!
//! Called from:
//! - DateTime and DateTimeImmutable declaration injection, and the name resolver / eval alias
//!   table, which desugar the procedural calls to it.
//!
//! Key details:
//! - php's procedural constructors answer `false` for a string they cannot parse, where the class
//!   constructor throws `DateMalformedStringException`. The desugaring used to be
//!   `new DateTime($s)`, so `date_create('bogus')` threw instead of returning `false`.
//! - The body is BUILT from AST nodes: `try { return new C($datetime, $timezone); } catch
//!   (Exception) { return false; }`. The declared `C|false` return lets `->format()` resolve on the
//!   success arm, as `createFromFormat()` does.

use super::*;
use crate::synthetic_class::{e_bool, e_new, e_var, s_return, s_try};

/// Builds the static `__elephc_date_create(string $datetime = "now", ?DateTimeZone $timezone =
/// null): class_name|false` factory for `class_name` (`"DateTime"` or `"DateTimeImmutable"`).
pub(super) fn datetime_date_create(class_name: &str) -> ClassMethod {
    let body = vec![s_try(
        vec![s_return(e_new(
            class_name,
            vec![e_var("datetime"), e_var("timezone")],
        ))],
        vec![(vec!["Exception"], None, vec![s_return(e_bool(false))])],
        None,
    )];
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_date_create".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "datetime".to_string(),
                Some(TypeExpr::Str),
                Some(Expr::new(ExprKind::StringLiteral("now".to_string()), dummy())),
                false,
            ),
            (
                "timezone".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
                    "DateTimeZone",
                ))))),
                Some(Expr::new(ExprKind::Null, dummy())),
                false,
            ),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Union(vec![
            TypeExpr::Named(Name::unqualified(class_name)),
            TypeExpr::False,
        ])),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}
