//! Purpose:
//! Recognizes an `assert($x instanceof C)` statement, after which `$x` is known to be a `C`.
//!
//! Called from:
//! - `crate::types::checker::stmt_check` (the checker's narrowing of the following statements).
//! - `crate::ir_lower::stmt` (the lowering's twin narrowing of the local's logical type).
//!
//! Key details:
//! - elephc's `assert()` always evaluates its assertion and throws `AssertionError` when it is
//!   false (`crate::assert_prelude`), so the edge after the statement only carries values that
//!   passed. Only an `instanceof` assertion narrows: it is the guard both the checker and the
//!   lowering narrow, so the two keep agreeing on the local's type.

use crate::parser::ast::{Expr, ExprKind};

/// Returns the `instanceof` condition an `assert(...)` expression statement proves.
pub(crate) fn asserted_instanceof_condition(expr: &Expr) -> Option<&Expr> {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    // In a namespace, an unqualified `assert(...)` keeps its global fallback until it is bound.
    let is_assert = |text: &str| text.trim_start_matches('\\').eq_ignore_ascii_case("assert");
    if !is_assert(name.as_str()) && !name.function_fallback().is_some_and(is_assert) {
        return None;
    }
    let condition = args.first()?;
    matches!(condition.kind, ExprKind::InstanceOf { .. }).then_some(condition)
}
