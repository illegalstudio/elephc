//! Purpose:
//! AST helpers shared by mutable and immutable DateTime setter methods.
//!
//! Called from:
//! - DateTime setter construction.
//!
//! Key details:
//! - Immutable paths mutate a clone of `$this`, so the receiver's class, timezone, microseconds
//!   and any subclass properties carry over to the result.

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
/// (`DateTimeImmutable`) clone `$this`, store the new timestamp on the clone and return it —
/// preserving copy-on-modify semantics and the receiver's (possibly subclass) identity.
pub(super) fn result_tail(result_ts: Expr, mutable: bool) -> Vec<Stmt> {
    result_tail_micro(result_ts, None, mutable)
}

/// Like `result_tail`, but with an explicit sub-second value for the result. When
/// `result_micro` is `None` the existing `$this->microsecond` is carried through
/// (the common case); add()/sub() pass the recomputed microsecond instead.
pub(super) fn result_tail_micro(
    result_ts: Expr,
    result_micro: Option<Expr>,
    mutable: bool,
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
            Stmt::assign("__new", clone_this()),
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "timestamp".to_string(),
                    value: result_ts,
                },
                dummy(),
            ),
            // The clone already carries the timezone; the sub-second value is stored explicitly
            // because add()/sub()/modify()/setTime() recompute it.
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
