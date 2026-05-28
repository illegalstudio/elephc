//! Purpose:
//! Regression tests for optimizer dce tries finally_paths behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Tests that statements following a `try/finally` are eliminated when the `finally` body
/// terminates with a return. The `finally` return dominates the post-try position, so any
/// statements placed after the `try/finally` block are unreachable and must be removed.
///
/// Input: `try { return 7; } finally { return 8; } echo 9;`
/// Expected: only the `try/finally` block remains; `echo 9` is eliminated.
#[test]
fn test_eliminate_dead_code_drops_statements_after_try_finally_exit() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "answer".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Try {
                        try_body: vec![Stmt::new(
                            StmtKind::Return(Some(Expr::int_lit(7))),
                            Span::dummy(),
                        )],
                        catches: Vec::new(),
                        finally_body: Some(vec![Stmt::new(
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
    assert_eq!(body.len(), 1);
    assert!(matches!(body[0].kind, StmtKind::Try { .. }));
}

/// Tests that an outer guard condition protecting a `try/finally` is preserved when only
/// unrelated local variables are modified inside the `try` body. Changing a variable other
/// than the guard condition does not make the guard side-effect-free, so the `if` guard
/// must be retained to ensure the `finally` block still executes under the correct condition.
///
/// Input: `if (flag) { try { other = 1; } finally { if (flag) echo 7 else echo 8; } }`
/// Expected: the outer `if (flag)` guard is preserved and the assignment plus the inner
/// conditional echo remain.
#[test]
fn test_eliminate_dead_code_preserves_outer_guard_for_finally_when_only_other_locals_change() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: Expr::var("flag"),
                    then_body: vec![Stmt::new(
                        StmtKind::Try {
                            try_body: vec![Stmt::assign("other", Expr::int_lit(1))],
                            catches: Vec::new(),
                            finally_body: Some(vec![Stmt::new(
                                StmtKind::If {
                                    condition: Expr::var("flag"),
                                    then_body: vec![Stmt::echo(Expr::int_lit(7))],
                                    elseif_clauses: Vec::new(),
                                    else_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
                                },
                                Span::dummy(),
                            )]),
                        },
                        Span::dummy(),
                    )],
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
    assert_eq!(
        then_body,
        &vec![Stmt::assign("other", Expr::int_lit(1)), Stmt::echo(Expr::int_lit(7))]
    );
}

/// Tests that when a `finally` block contains no return or exit path, statements placed
/// after the `try/finally` can sink into the finally path. Because the `finally` body does
/// not dominate the post-try position, the post-try statements are reachable and must be
/// retained, and the optimizer may merge them with the finally path.
///
/// Input: `try { echo 7; } finally { echo 8; } echo 9;`
/// Expected: all three echo statements are retained in order.
#[test]
fn test_eliminate_dead_code_sinks_tail_into_safe_finally_path() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: vec![
                Stmt::new(
                    StmtKind::Try {
                        try_body: vec![Stmt::echo(Expr::int_lit(7))],
                        catches: Vec::new(),
                        finally_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
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
    assert_eq!(body, &vec![Stmt::echo(Expr::int_lit(7)), Stmt::echo(Expr::int_lit(8)), Stmt::echo(Expr::int_lit(9))]);
}
