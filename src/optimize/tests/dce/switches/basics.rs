//! Purpose:
//! Regression tests for optimizer dce switches basics behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Verifies DCE removes a switch with no side-effect subject and all-empty cases, replacing it with the subject expression.
#[test]
fn test_eliminate_dead_code_drops_empty_switch_shell_created_by_branch_dce() {
    let touch = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let pure_builtin = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("strlen"),
            args: vec![Expr::string_lit("abc")],
        },
        Span::dummy(),
    );
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::Switch {
                    subject: touch.clone(),
                    cases: vec![(
                        vec![Expr::int_lit(1)],
                        vec![
                            Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy()),
                            Stmt::new(StmtKind::Break(1), Span::dummy()),
                        ],
                    )],
                    default: None,
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
    assert_eq!(body.len(), 1);
    assert_eq!(
        body[0],
        Stmt::new(StmtKind::ExprStmt(touch), Span::dummy()),
    );
}
