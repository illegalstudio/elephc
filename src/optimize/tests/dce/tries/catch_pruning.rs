//! Purpose:
//! Regression tests for optimizer dce tries catch_pruning behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Verifies that DCE drops all catch clauses when the try body cannot throw.
///
/// Non-throwing try bodies make all catch clauses unreachable dead code.
/// The finally block (if present) is preserved.
#[test]
fn test_eliminate_dead_code_drops_unreachable_catches_after_non_throwing_try() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::echo(Expr::int_lit(7))],
                    catches: vec![crate::parser::ast::CatchClause {
                        exception_types: vec!["Exception".into()],
                        variable: Some("e".into()),
                        body: vec![Stmt::echo(Expr::int_lit(9))],
                    }],
                    finally_body: None,
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
    assert_eq!(body, &vec![Stmt::echo(Expr::int_lit(7))]);
}

/// Verifies that DCE drops all catch clauses when the try body cannot throw,
/// even when a finally block is present.
///
/// The finally block executes regardless of whether an exception occurs,
/// so it must be preserved even when catches are unreachable.
#[test]
fn test_eliminate_dead_code_drops_unreachable_catches_before_finally() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::echo(Expr::int_lit(7))],
                    catches: vec![crate::parser::ast::CatchClause {
                        exception_types: vec!["Exception".into()],
                        variable: Some("e".into()),
                        body: vec![Stmt::echo(Expr::int_lit(9))],
                    }],
                    finally_body: Some(vec![Stmt::echo(Expr::int_lit(8))]),
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
    assert_eq!(body, &vec![Stmt::echo(Expr::int_lit(7)), Stmt::echo(Expr::int_lit(8))]);
}

/// Verifies that when a Throwable catch appears before a more specific type,
/// the more specific catch is pruned as shadowed dead code.
///
/// Throwable is the root of PHP's exception hierarchy; any catch for a
/// subclass is unconditionally shadowed and unreachable.
#[test]
fn test_eliminate_dead_code_drops_catches_shadowed_by_throwable() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(
                        StmtKind::Throw(Expr::string_lit("boom")),
                        Span::dummy(),
                    )],
                    catches: vec![
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Throwable".into()],
                            variable: Some("t".into()),
                            body: vec![Stmt::echo(Expr::int_lit(7))],
                        },
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::echo(Expr::int_lit(8))],
                        },
                    ],
                    finally_body: None,
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
    let StmtKind::Try { catches, .. } = &body[0].kind else {
        panic!("expected try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(catches[0].exception_types.len(), 1);
    assert_eq!(catches[0].exception_types[0].as_str(), "Throwable");
    assert_eq!(catches[0].variable.as_deref(), Some("t"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}

/// Verifies that when multiple consecutive catches have identical exception types,
/// all but the first are pruned as shadowed dead code.
///
/// PHP uses first-match semantics; later catches with the same type are unreachable.
#[test]
fn test_eliminate_dead_code_drops_duplicate_shadowed_catch_types() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(
                        StmtKind::Throw(Expr::string_lit("boom")),
                        Span::dummy(),
                    )],
                    catches: vec![
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("first".into()),
                            body: vec![Stmt::echo(Expr::int_lit(7))],
                        },
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("second".into()),
                            body: vec![Stmt::echo(Expr::int_lit(8))],
                        },
                    ],
                    finally_body: None,
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
    let StmtKind::Try { catches, .. } = &body[0].kind else {
        panic!("expected try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(catches[0].exception_types.len(), 1);
    assert_eq!(catches[0].exception_types[0].as_str(), "Exception");
    assert_eq!(catches[0].variable.as_deref(), Some("first"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}

/// Verifies that when pruning shadowed catches exposes identical adjacent catches,
/// DCE merges them into a single catch with multiple exception types.
///
/// After dropping the shadowed Exception catch (duplicate of the first),
/// the remaining Error and Exception catches have identical bodies and must
/// be merged into a single catch clause listing both types.
#[test]
fn test_eliminate_dead_code_merges_identical_catches_exposed_by_shadow_drop() {
    let program = vec![Stmt::new(
        StmtKind::FunctionDecl {
            name: "main".into(),
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(
                        StmtKind::Throw(Expr::string_lit("boom")),
                        Span::dummy(),
                    )],
                    catches: vec![
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::echo(Expr::int_lit(7))],
                        },
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Exception".into()],
                            variable: Some("shadowed".into()),
                            body: vec![Stmt::echo(Expr::int_lit(8))],
                        },
                        crate::parser::ast::CatchClause {
                            exception_types: vec!["Error".into()],
                            variable: Some("e".into()),
                            body: vec![Stmt::echo(Expr::int_lit(7))],
                        },
                    ],
                    finally_body: None,
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
    let StmtKind::Try { catches, .. } = &body[0].kind else {
        panic!("expected try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(catches[0].variable.as_deref(), Some("e"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
    assert_eq!(catches[0].exception_types.len(), 2);
    assert_eq!(catches[0].exception_types[0].as_str(), "Error");
    assert_eq!(catches[0].exception_types[1].as_str(), "Exception");
}
