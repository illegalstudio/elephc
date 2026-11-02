//! Purpose:
//! AST-built `DateInterval::__serialize()` and `DateInterval::__unserialize()` methods.
//!
//! Called from:
//! - DateInterval declaration injection.
//!
//! Key details:
//! - The array shape is php 8.2's: the nine public components plus `from_string`.
//! - Libraries test `method_exists(DateInterval::class, '__unserialize')` to tell php >= 8.2 from
//!   older versions; without the method Carbon took its php 8.1 path and wrote the removed
//!   `weekday`/`special_type` properties, a dynamic-property deprecation per interval.

use super::*;

const INTERVAL_INT_COMPONENTS: [&str; 7] = ["y", "m", "d", "h", "i", "s", "invert"];

fn var(name: &str) -> Expr {
    Expr::new(ExprKind::Variable(name.to_string()), dummy())
}

fn str_lit(value: &str) -> Expr {
    Expr::new(ExprKind::StringLiteral(value.to_string()), dummy())
}

/// `$data["key"]`.
fn data_entry(key: &str) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess { array: Box::new(var("data")), index: Box::new(str_lit(key)) },
        dummy(),
    )
}

/// `$data["key"] ?? <default>`.
fn data_entry_or(key: &str, default: Expr) -> Expr {
    Expr::new(
        ExprKind::NullCoalesce { value: Box::new(data_entry(key)), default: Box::new(default) },
        dummy(),
    )
}

fn cast(target: crate::parser::ast::CastType, expr: Expr) -> Expr {
    Expr::new(ExprKind::Cast { target, expr: Box::new(expr) }, dummy())
}

/// `DateInterval::__serialize(): array` — the components keyed by name, as php 8.2 returns them.
pub(super) fn date_interval_serialize() -> ClassMethod {
    let mut entries: Vec<(Expr, Expr)> = ["y", "m", "d", "h", "i", "s", "f", "invert", "days"]
        .into_iter()
        .map(|name| (str_lit(name), this_property(name)))
        .collect();
    entries.push((str_lit("from_string"), Expr::new(ExprKind::BoolLiteral(false), dummy())));
    method(
        "__serialize",
        Vec::new(),
        Some(TypeExpr::Array(Box::new(TypeExpr::Named(Name::unqualified("mixed"))))),
        vec![return_expr(Expr::new(ExprKind::ArrayLiteralAssoc(entries), dummy()))],
    )
}

/// `DateInterval::__unserialize(array $data): void` — restores each component, a missing one
/// reading as zero (and `days` as `false`).
pub(super) fn date_interval_unserialize() -> ClassMethod {
    use crate::parser::ast::CastType;
    let zero = || Expr::new(ExprKind::IntLiteral(0), dummy());
    let mut body: Vec<Stmt> = INTERVAL_INT_COMPONENTS
        .into_iter()
        .map(|name| assign_this_property(name, cast(CastType::Int, data_entry_or(name, zero()))))
        .collect();
    body.push(assign_this_property(
        "f",
        cast(
            CastType::Float,
            data_entry_or("f", Expr::new(ExprKind::FloatLiteral(0.0), dummy())),
        ),
    ));
    let days_is_int = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("is_int"),
            args: vec![data_entry_or("days", Expr::new(ExprKind::BoolLiteral(false), dummy()))],
        },
        dummy(),
    );
    body.push(assign_this_property(
        "days",
        Expr::new(
            ExprKind::Ternary {
                condition: Box::new(days_is_int),
                then_expr: Box::new(cast(CastType::Int, data_entry("days"))),
                else_expr: Box::new(Expr::new(ExprKind::BoolLiteral(false), dummy())),
            },
            dummy(),
        ),
    ));
    method(
        "__unserialize",
        vec![("data".to_string(), Some(TypeExpr::Array(Box::new(TypeExpr::Named(Name::unqualified("mixed"))))), None, false)],
        Some(TypeExpr::Void),
        body,
    )
}
