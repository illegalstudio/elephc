//! Purpose:
//! Gives every cloned dispatch arm its own span identities.
//!
//! Called from:
//! - `super::rewrite` and `super::builtins` for each arm they build from a shared template.
//!
//! Key details:
//! - Spans are map keys, not just coordinates: the checker records builtin call types, local
//!   storage decisions and reference bindings by span (and name), and EIR lowering reads them
//!   back. Two clones of one statement would otherwise share every key, so a type or decision
//!   recorded for one arm (`count($months)`) would be applied to another (`count($weekdays)`).
//! - Only `end_line` changes, to a process-unique value; the start line/column that
//!   diagnostics print and the source identity packed into `end_col` are kept.
//! - Within ONE call, equal spans stay equal (a per-call map), so nodes the parser deliberately
//!   tied together by span keep matching inside the clone.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

use super::walk::{walk_expr_children, walk_stmt_children, Visitor};

/// First `end_line` handed to a respanned node; far above any real or synthetic line.
const RESPAN_END_LINE_BASE: u32 = 3_000_000_000;

/// Process-wide counter, so respanned spans never collide across scopes or arms.
static NEXT_RESPAN: AtomicU32 = AtomicU32::new(0);

struct Respan {
    map: HashMap<Span, Span>,
}

impl Respan {
    fn fresh(&mut self, span: Span) -> Span {
        if !span.identifies_a_node() {
            return span;
        }
        *self.map.entry(span).or_insert_with(|| {
            let index = NEXT_RESPAN.fetch_add(1, Ordering::Relaxed);
            Span {
                end_line: RESPAN_END_LINE_BASE.saturating_add(index),
                ..span
            }
        })
    }
}

impl Visitor for Respan {
    fn stmt(&mut self, stmt: &mut Stmt) {
        stmt.span = self.fresh(stmt.span);
        if let StmtKind::FunctionDecl { body, .. } = &mut stmt.kind {
            self.block(body);
        }
        walk_stmt_children(self, stmt);
    }

    fn expr(&mut self, expr: &mut Expr) {
        expr.span = self.fresh(expr.span);
        if let ExprKind::Closure { params, body, .. } = &mut expr.kind {
            for (_, _, default, _) in params.iter_mut() {
                if let Some(default) = default {
                    self.expr(default);
                }
            }
            self.block(body);
        }
        walk_expr_children(self, expr);
    }
}

/// Re-identifies every span in `stmts` (one consistent mapping for the whole slice).
pub(super) fn respan_stmts(stmts: &mut Vec<Stmt>) {
    Respan {
        map: HashMap::new(),
    }
    .block(stmts);
}

/// Re-identifies every span in one statement.
pub(super) fn respan_stmt(stmt: &mut Stmt) {
    Respan {
        map: HashMap::new(),
    }
    .stmt(stmt);
}

/// Re-identifies every span in `expr`.
pub(super) fn respan_expr(expr: &mut Expr) {
    Respan {
        map: HashMap::new(),
    }
    .expr(expr);
}
