//! Purpose:
//! Preserves PHP generator classification while allowing callable bodies to be optimized.
//!
//! Called from:
//! - Callable body rewrites in folding, propagation, pruning, and dead-code elimination.
//!
//! Key details:
//! - PHP classifies a generator syntactically, including unreachable yields.
//! - Pre-check folding needs a syntactic marker; later passes preserve the same invariant.
//! - Removing the final yield adds one unreachable marker instead of restoring the whole body.

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

/// Rewrites a callable body and retains a minimal dead yield if its last yield disappears.
pub(super) fn rewrite_preserving_yield<F>(body: Vec<Stmt>, rewrite: F) -> Vec<Stmt>
where
    F: FnOnce(Vec<Stmt>) -> Vec<Stmt>,
{
    if !crate::types::checker::yield_validation::body_contains_yield(&body) {
        return rewrite(body);
    }
    let source_mode = body[0].source_mode;
    let strict_types = body[0].strict_types;
    let mut rewritten = rewrite(body);
    if !crate::types::checker::yield_validation::body_contains_yield(&rewritten) {
        let yield_span = Span::synthetic();
        let yield_stmt = Stmt {
            kind: StmtKind::ExprStmt(Expr {
                kind: ExprKind::Yield { key: None, value: None },
                span: yield_span,
            }),
            span: yield_span,
            source_mode,
            strict_types,
            attributes: Vec::new(),
        };
        rewritten.push(Stmt {
            kind: StmtKind::If {
                condition: Expr { kind: ExprKind::BoolLiteral(false), span: Span::synthetic() },
                then_body: vec![yield_stmt],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            span: Span::synthetic(),
            source_mode,
            strict_types,
            attributes: Vec::new(),
        });
    }
    rewritten
}
