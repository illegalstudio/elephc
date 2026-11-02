//! Purpose:
//! AST helpers shared by mutable and immutable DateTime setter methods.
//!
//! Called from:
//! - DateTime setter construction.
//!
//! Key details:
//! - Immutable paths create fresh instances while carrying timezone and microseconds.

use super::*;

/// Builds `(int) date($fmt, $this->timestamp)` — extracts a numeric component of the stored time.
pub(super) fn date_component_int(fmt: &str) -> Expr {
    Expr::new(
        ExprKind::Cast {
            target: crate::parser::ast::CastType::Int,
            expr: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("date"),
                    args: vec![
                        Expr::new(ExprKind::StringLiteral(fmt.to_string()), dummy()),
                        this_property("timestamp"),
                    ],
                },
                dummy(),
            )),
        },
        dummy(),
    )
}

/// Builds an `__elephc_mktime_raw(hour, minute, second, month, day, year)` call expression — the
/// internal fixed-arity runtime entry that the `mktime()`/`gmmktime()` procedural aliases desugar
/// to. Synthetic method bodies call it directly (they are injected after the name resolver, so
/// the alias rewrite never runs on them); using the raw name avoids an unresolved `mktime` call.
pub(super) fn mktime_call(parts: [&str; 6]) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("__elephc_mktime_raw"),
            args: parts
                .iter()
                .map(|n| Expr::new(ExprKind::Variable((*n).to_string()), dummy()))
                .collect(),
        },
        dummy(),
    )
}

/// Builds the statement tail that publishes a freshly computed timestamp.
///
/// Mutable classes (`DateTime`) assign `$this->timestamp` and return `$this`. Immutable classes
/// (`DateTimeImmutable`) construct a fresh instance, copy the new timestamp and the timezone name,
/// and return it — preserving copy-on-modify semantics.
pub(super) fn result_tail(result_ts: Expr, mutable: bool, class_name: &str) -> Vec<Stmt> {
    result_tail_micro(result_ts, None, mutable, class_name)
}

/// Like `result_tail`, but with an explicit sub-second value for the result. When
/// `result_micro` is `None` the existing `$this->microsecond` is carried through
/// (the common case); add()/sub() pass the recomputed microsecond instead.
pub(super) fn result_tail_micro(
    result_ts: Expr,
    result_micro: Option<Expr>,
    mutable: bool,
    _class_name: &str,
) -> Vec<Stmt> {
    let micro = result_micro.unwrap_or_else(|| this_property("microsecond"));
    if mutable {
        vec![
            assign_this_property("microsecond", micro),
            assign_this_property("timestamp", result_ts),
            return_expr(Expr::new(ExprKind::This, dummy())),
        ]
    } else {
        let new_var = || Expr::new(ExprKind::Variable("__new".to_string()), dummy());
        vec![
            // php clones the receiver: a subclass (CarbonImmutable) keeps its class and its own
            // properties, and its `__clone` runs.
            Stmt::assign(
                "__new",
                Expr::new(ExprKind::Clone(Box::new(Expr::new(ExprKind::This, dummy()))), dummy()),
            ),
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "timestamp".to_string(),
                    value: result_ts,
                },
                dummy(),
            ),
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "timezone_name".to_string(),
                    value: this_property("timezone_name"),
                },
                dummy(),
            ),
            // Carry the sub-second component into the fresh immutable instance so it survives
            // setTimestamp/setTime/setDate/setTimezone/add/sub/modify.
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "microsecond".to_string(),
                    value: micro,
                },
                dummy(),
            ),
            return_expr(new_var()),
        ]
    }
}
