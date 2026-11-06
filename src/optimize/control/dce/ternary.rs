//! Purpose:
//! Normalizes discarded ternaries before conditional tail specialization.
//!
//! Called from:
//! - The DCE statement-block loop.
//!
//! Key details:
//! - Conditions and selected arms keep their original expressions and spans.
//! - Existing tail-cloning guards apply before the resulting conditional is specialized.

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};

/// Exposes a discarded ternary's paths to the same guarded tail sinking used by an `if`.
pub(super) fn discarded_ternary_as_if(stmt: Stmt) -> Stmt {
    let source_mode = stmt.source_mode;
    let strict_types = stmt.strict_types;
    match stmt.kind {
        StmtKind::ExprStmt(Expr {
            kind: ExprKind::Ternary { condition, then_expr, else_expr }, ..
        }) => {
            let arm = |expr: Expr| Stmt {
                span: expr.span,
                kind: StmtKind::ExprStmt(expr),
                source_mode,
                strict_types,
                attributes: Vec::new(),
            };
            Stmt {
                kind: StmtKind::If {
                    condition: *condition,
                    then_body: vec![arm(*then_expr)],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![arm(*else_expr)]),
                },
                ..stmt
            }
        }
        kind => Stmt { kind, ..stmt },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Consecutive discarded ternaries must not duplicate the remaining control-flow tail.
    #[test]
    fn discarded_ternary_tail_keeps_ast_linear() {
        let mut source = String::from(
            "<?php function stress(int $argc): void { $left = 0; $right = 0;",
        );
        for value in 0..256 {
            source.push_str(&format!("$argc > {value} ? ++$left : ++$right;"));
        }
        source.push_str("echo $left + $right; }");
        let tokens = crate::lexer::tokenize(&source).expect("tokenize discarded ternary fixture");
        let mut statements = crate::parser::parse(&tokens).expect("parse discarded ternary fixture");
        let StmtKind::FunctionDecl { body, .. } = statements.remove(0).kind else {
            panic!("expected function declaration fixture");
        };
        let eliminated = super::super::dce_block(body);
        assert_eq!(eliminated.len(), 258);
        assert_eq!(eliminated.iter().filter(|stmt| matches!(stmt.kind, StmtKind::If { .. })).count(), 256);
    }
}
