//! Purpose:
//! Regression tests for optimizer dce basics behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Verifies that a strict-bool guard (`flag === true`) inside an outer if-body
/// is NOT eliminated when the outer condition uses the same guard expression.
/// A local assignment to `flag` inside the outer if-body invalidates the guard
/// assumption for the nested if, so the inner conditional must remain live.
#[test]
fn test_eliminate_dead_code_invalidates_outer_strict_bool_guard_after_local_write() {
    let strict_true = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::var("flag")),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::BoolLiteral(true), Span::dummy())),
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: strict_true.clone(),
                    then_body: vec![
                        Stmt::assign("flag", Expr::new(ExprKind::BoolLiteral(false), Span::dummy())),
                        Stmt::new(
                            StmtKind::If {
                                condition: strict_true,
                                then_body: vec![Stmt::echo(Expr::int_lit(8))],
                                elseif_clauses: Vec::new(),
                                else_body: Some(vec![Stmt::echo(Expr::int_lit(7))]),
                            },
                            Span::dummy(),
                        ),
                    ],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                Span::dummy(),
            )],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    let StmtKind::If { then_body, .. } = &body[0].kind else {
        panic!("expected if");
    };
    let StmtKind::If { .. } = &then_body[1].kind else {
        panic!("expected strict inner if to remain after guard invalidation");
    };
}
