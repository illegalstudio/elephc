//! Purpose:
//! AST-built `DateTime(Immutable)::__serialize()` and `__unserialize()` methods.
//!
//! Called from:
//! - DateTime and DateTimeImmutable declaration injection.
//!
//! Key details:
//! - php 8.2 serializes a date as `["date" => "Y-m-d H:i:s.u", "timezone_type" => 1|3,
//!   "timezone" => name]`; `__serialize()` returns that shape (type 1 for an offset zone, 3 for
//!   an identifier, measured with `php -n` 8.5).
//! - `__unserialize()` takes that shape, and also elephc's own backing properties
//!   (`timestamp`, `timezone_name`, `microsecond`), which is what `(array) $date` yields here:
//!   symfony/clock's `DatePoint::__construct()` ends with `$this->__unserialize((array) $now)`,
//!   and without the method the whole class was interpreted.

use super::*;
use crate::parser::ast::{BinOp, CastType};
use crate::synthetic_class::{
    e_binop, e_call, e_cast, e_index, e_method_call, e_new, e_null_coalesce, e_str, e_this_prop,
    e_var, s_if, s_prop_assign, s_return_void,
};

fn data(key: &str) -> Expr {
    e_index(e_var("data"), e_str(key))
}

fn this_assign(property: &str, value: Expr) -> Stmt {
    s_prop_assign(crate::synthetic_class::e_this(), property, value)
}

fn array_mixed() -> TypeExpr {
    TypeExpr::Array(Box::new(TypeExpr::Named(Name::unqualified("mixed"))))
}

/// `__serialize(): array` -- php 8.2's three-entry shape.
pub(super) fn datetime_serialize() -> ClassMethod {
    let zone = e_this_prop("timezone_name");
    let first = e_call("substr", vec![zone.clone(), Expr::new(ExprKind::IntLiteral(0), dummy()), Expr::new(ExprKind::IntLiteral(1), dummy())]);
    let is_offset = e_binop(
        e_binop(first.clone(), BinOp::StrictEq, e_str("+")),
        BinOp::Or,
        e_binop(first, BinOp::StrictEq, e_str("-")),
    );
    let zone_type = Expr::new(
        ExprKind::Ternary {
            condition: Box::new(is_offset),
            then_expr: Box::new(Expr::new(ExprKind::IntLiteral(1), dummy())),
            else_expr: Box::new(Expr::new(ExprKind::IntLiteral(3), dummy())),
        },
        dummy(),
    );
    let entries = vec![
        (
            e_str("date"),
            e_method_call(crate::synthetic_class::e_this(), "format", vec![e_str("Y-m-d H:i:s.u")]),
        ),
        (e_str("timezone_type"), zone_type),
        (e_str("timezone"), zone),
    ];
    method(
        "__serialize",
        Vec::new(),
        Some(array_mixed()),
        vec![return_expr(Expr::new(ExprKind::ArrayLiteralAssoc(entries), dummy()))],
    )
}

/// `__unserialize(array $data): void` -- php's shape, or elephc's backing properties.
pub(super) fn datetime_unserialize() -> ClassMethod {
    let backing = vec![
        this_assign("timestamp", e_cast(CastType::Int, data("timestamp"))),
        this_assign(
            "timezone_name",
            e_cast(CastType::String, e_null_coalesce(data("timezone_name"), e_str("UTC"))),
        ),
        this_assign(
            "microsecond",
            e_cast(
                CastType::Int,
                e_null_coalesce(data("microsecond"), Expr::new(ExprKind::IntLiteral(0), dummy())),
            ),
        ),
        s_return_void(),
    ];
    let source = || e_var("source");
    let body = vec![
        s_if(
            e_call("isset", vec![data("timestamp")]),
            backing,
            vec![],
            None,
        ),
        crate::synthetic_class::s_assign(
            "source",
            e_new(
                "DateTimeImmutable",
                vec![
                    e_cast(CastType::String, data("date")),
                    e_new("DateTimeZone", vec![e_cast(CastType::String, data("timezone"))]),
                ],
            ),
        ),
        this_assign("timestamp", e_method_call(source(), "getTimestamp", vec![])),
        this_assign(
            "timezone_name",
            e_method_call(e_method_call(source(), "getTimezone", vec![]), "getName", vec![]),
        ),
        this_assign(
            "microsecond",
            e_cast(CastType::Int, e_method_call(source(), "format", vec![e_str("u")])),
        ),
    ];
    method(
        "__unserialize",
        vec![("data".to_string(), Some(array_mixed()), None, false)],
        Some(TypeExpr::Void),
        body,
    )
}
