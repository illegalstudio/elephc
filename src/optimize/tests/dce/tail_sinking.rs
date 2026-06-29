//! Purpose:
//! Regression tests for optimizer dce tail_sinking behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Verifies that when an if/elseif/else chain has identical effectful bodies (both calling
/// pure builtins), the elseif collapses into a negated condition on the first branch,
/// eliminating the second effectful call and reducing the chain to a single conditional check.
/// The elseif condition `tap()` is preserved as the else-branch body, and `touch()` becomes
/// the negated condition of the outer if.
#[test]
fn test_eliminate_dead_code_reduces_empty_if_chain_to_needed_condition_checks() {
    let touch = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("touch"),
            args: Vec::new(),
        },
        Span::dummy(),
    );
    let tap = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("tap"),
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
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: touch.clone(),
                    then_body: vec![Stmt::new(StmtKind::ExprStmt(pure_builtin.clone()), Span::dummy())],
                    elseif_clauses: vec![(
                        tap.clone(),
                        vec![Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy())],
                    )],
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
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::If {
                condition: Expr::new(ExprKind::Not(Box::new(touch)), Span::dummy()),
                then_body: vec![Stmt::new(StmtKind::ExprStmt(tap), Span::dummy())],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )]
    );
}

/// Verifies that when an if statement with an explicit else is followed by a statement
/// (`echo 9`), that trailing statement sinks into the else branch (since the if then-branch
/// terminates with return). The else body becomes `[echo 8, echo 9]`.
#[test]
fn test_eliminate_dead_code_sinks_tail_into_if_fallthrough_branch() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![
                Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("flag"),
                        then_body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(7))),
                            Span::dummy(),
                        )],
                        elseif_clauses: Vec::new(),
                        else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::If {
                condition: Expr::var("flag"),
                then_body: vec![Stmt::new(
                    StmtKind::Return(Some(Expr::int_lit(7))),
                    Span::dummy(),
                )],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::echo(Expr::int_lit(8)), Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )]
    );
}

/// Verifies that when an if without an else is followed by a statement (`echo 9`), and the
/// then-branch terminates with return, the trailing statement sinks into a synthesized else
/// branch. The result is an if with `else_body: Some([echo 9])`.
#[test]
fn test_eliminate_dead_code_sinks_tail_into_implicit_else_path() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![
                Stmt::new(
                    StmtKind::If {
                        condition: Expr::var("flag"),
                        then_body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(7))),
                            Span::dummy(),
                        )],
                        elseif_clauses: Vec::new(),
                        else_body: None,
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::If {
                condition: Expr::var("flag"),
                then_body: vec![Stmt::new(
                    StmtKind::Return(Some(Expr::int_lit(7))),
                    Span::dummy(),
                )],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![Stmt::echo(Expr::int_lit(9))]),
            },
            Span::dummy(),
        )]
    );
}

/// Verifies that when an IfDef with a return in the else branch is followed by a statement
/// (`echo 9`), that trailing statement sinks into the then branch of the IfDef (since the
/// else branch terminates with return). The then_body becomes `[echo 7, echo 9]` and the
/// else branch remains `[return 8]`.
#[test]
fn test_eliminate_dead_code_sinks_tail_into_ifdef_fallthrough_paths() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![
                Stmt::new(
                    StmtKind::IfDef {
                        symbol: "DEBUG".into(),
                        then_body: vec![Stmt::echo(Expr::int_lit(7))],
                        else_body: Some(vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(8))),
                            Span::dummy(),
                        )]),
                    },
                    Span::dummy(),
                ),
                Stmt::echo(Expr::int_lit(9)),
            ],
        },
        Span::dummy(),
    )];

    let eliminated = eliminate_dead_code(program);

    let StmtKind::FunctionDecl { body, .. } = &eliminated[0].kind else {
        panic!("expected function");
    };
    assert_eq!(
        body,
        &vec![Stmt::new(
            StmtKind::IfDef {
                symbol: "DEBUG".into(),
                then_body: vec![Stmt::echo(Expr::int_lit(7)), Stmt::echo(Expr::int_lit(9))],
                else_body: Some(vec![Stmt::new(
                    StmtKind::Return(Some(Expr::int_lit(8))),
                    Span::dummy(),
                )]),
            },
            Span::dummy(),
        )]
    );
}

/// Verifies that when both branches of an if statement have identical pure effectful bodies
/// (same `strlen` call), the if collapses to just the condition expression (`touch()`),
/// dropping both branches entirely. Pure calls with no side effects are dead code.
#[test]
fn test_eliminate_dead_code_reduces_empty_if_to_effectful_condition_eval() {
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
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: touch.clone(),
                    then_body: vec![Stmt::new(StmtKind::ExprStmt(pure_builtin.clone()), Span::dummy())],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![Stmt::new(StmtKind::ExprStmt(pure_builtin), Span::dummy())]),
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
