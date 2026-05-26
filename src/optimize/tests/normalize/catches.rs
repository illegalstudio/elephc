//! Purpose:
//! Regression tests for optimizer normalize catches behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

#[test]
    // Verifies that two adjacent catch clauses with identical bodies but different
    // exception types are merged into a single clause with both exception types combined.
fn test_normalize_control_flow_merges_adjacent_identical_catches() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::Throw(Box::new(Expr::new(
                        ExprKind::NewObject {
                            class_name: Name::unqualified("Exception"),
                            args: Vec::new(),
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                )),
                Span::dummy(),
            )],
            catches: vec![
                crate::parser::ast::CatchClause {
                    exception_types: vec![Name::unqualified("A")],
                    variable: Some("e".into()),
                    body: vec![Stmt::echo(Expr::int_lit(7))],
                },
                crate::parser::ast::CatchClause {
                    exception_types: vec![Name::unqualified("B")],
                    variable: Some("e".into()),
                    body: vec![Stmt::echo(Expr::int_lit(7))],
                },
            ],
            finally_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    let StmtKind::Try { catches, .. } = &pruned[0].kind else {
        panic!("expected normalized try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(
        catches[0].exception_types,
        vec![Name::unqualified("A"), Name::unqualified("B")]
    );
    assert_eq!(catches[0].variable.as_deref(), Some("e"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
    // Verifies that when two catch clauses with overlapping exception types are merged,
    // duplicate exception types are removed while preserving all unique types.
fn test_normalize_control_flow_deduplicates_merged_catch_exception_types() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::Throw(Box::new(Expr::new(
                        ExprKind::NewObject {
                            class_name: Name::unqualified("Exception"),
                            args: Vec::new(),
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                )),
                Span::dummy(),
            )],
            catches: vec![
                crate::parser::ast::CatchClause {
                    exception_types: vec![Name::unqualified("A"), Name::unqualified("B")],
                    variable: Some("e".into()),
                    body: vec![Stmt::echo(Expr::int_lit(7))],
                },
                crate::parser::ast::CatchClause {
                    exception_types: vec![Name::unqualified("B"), Name::unqualified("C")],
                    variable: Some("e".into()),
                    body: vec![Stmt::echo(Expr::int_lit(7))],
                },
            ],
            finally_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    let StmtKind::Try { catches, .. } = &pruned[0].kind else {
        panic!("expected normalized try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(
        catches[0].exception_types,
        vec![
            Name::unqualified("A"),
            Name::unqualified("B"),
            Name::unqualified("C")
        ]
    );
    assert_eq!(catches[0].variable.as_deref(), Some("e"));
    assert_eq!(catches[0].body, vec![Stmt::echo(Expr::int_lit(7))]);
}

#[test]
    // Verifies that exception types within a merged catch clause are sorted alphabetically
    // to produce deterministic ordering regardless of input sequence.
fn test_normalize_control_flow_sorts_catch_exception_types() {
    let program = vec![Stmt::new(
        StmtKind::Try {
            try_body: vec![Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::Throw(Box::new(Expr::new(
                        ExprKind::NewObject {
                            class_name: Name::unqualified("Exception"),
                            args: Vec::new(),
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                )),
                Span::dummy(),
            )],
            catches: vec![crate::parser::ast::CatchClause {
                exception_types: vec![
                    Name::unqualified("Zed"),
                    Name::unqualified("Alpha"),
                    Name::unqualified("Mid"),
                ],
                variable: Some("e".into()),
                body: vec![Stmt::echo(Expr::int_lit(7))],
            }],
            finally_body: None,
        },
        Span::dummy(),
    )];

    let pruned = normalize_control_flow(program);

    assert_eq!(pruned.len(), 1);
    let StmtKind::Try { catches, .. } = &pruned[0].kind else {
        panic!("expected normalized try");
    };
    assert_eq!(catches.len(), 1);
    assert_eq!(
        catches[0].exception_types,
        vec![
            Name::unqualified("Alpha"),
            Name::unqualified("Mid"),
            Name::unqualified("Zed")
        ]
    );
}
