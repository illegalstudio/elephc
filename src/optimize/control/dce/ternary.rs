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

    /// Tail specialization keeps the condition and arm profiles of the physical source file.
    #[test]
    fn discarded_ternary_tail_retains_statement_source_profile() {
        use crate::source::{SourceMode, SourceProfile};

        let tokens = crate::lexer::tokenize(
            "<?php $argc > 1 ? printf('then') : printf('else'); echo 'tail';",
        ).expect("tokenize ternary source-profile fixture");
        let mut statements = crate::parser::parse(&tokens).expect("parse ternary source-profile fixture");
        let expected = SourceProfile { mode: SourceMode::Lfc, strict_types: true };
        for stmt in &mut statements {
            stmt.source_mode = expected.mode;
            stmt.strict_types = expected.strict_types;
        }
        let eliminated = crate::source::with_parse_mode(SourceProfile::new(SourceMode::Php), || {
            super::super::dce_block(statements)
        });
        assert_eq!(eliminated.len(), 1);
        assert_eq!(eliminated[0].profile(), expected);
        let StmtKind::If { then_body, else_body: Some(else_body), .. } = &eliminated[0].kind else {
            panic!("expected a tail-specialized ternary");
        };
        assert!(!then_body.is_empty() && !else_body.is_empty());
        for stmt in then_body.iter().chain(else_body) {
            assert_eq!(stmt.profile(), expected);
        }
    }

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
