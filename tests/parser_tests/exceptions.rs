//! Purpose:
//! Integration or regression tests for parser AST coverage of exceptions, including try catch finally, multi catch, and catch without variable.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_parse_try_catch_finally() {
    let stmts = parse_source(
        "<?php try { throw $e; } catch (MyException $err) { echo 1; } finally { echo 2; }",
    );
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![Stmt::new(
                    StmtKind::Throw(Expr::var("e")),
                    elephc::span::Span::dummy(),
                )],
                catches: vec![CatchClause {
                    exception_types: vec!["MyException".into()],
                    variable: Some("err".into()),
                    body: vec![Stmt::echo(Expr::int_lit(1))],
                }],
                finally_body: Some(vec![Stmt::echo(Expr::int_lit(2))]),
            },
            elephc::span::Span::dummy(),
        )]
    );
}

#[test]
fn test_parse_multi_catch() {
    let stmts = parse_source(
        "<?php try { throw $e; } catch (FooException | BarException $err) { echo 1; }",
    );
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![Stmt::new(
                    StmtKind::Throw(Expr::var("e")),
                    elephc::span::Span::dummy(),
                )],
                catches: vec![CatchClause {
                    exception_types: vec!["FooException".into(), "BarException".into()],
                    variable: Some("err".into()),
                    body: vec![Stmt::echo(Expr::int_lit(1))],
                }],
                finally_body: None,
            },
            elephc::span::Span::dummy(),
        )]
    );
}

#[test]
fn test_parse_catch_without_variable() {
    let stmts = parse_source("<?php try { throw $e; } catch (Exception) { echo 1; }");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::Try {
                try_body: vec![Stmt::new(
                    StmtKind::Throw(Expr::var("e")),
                    elephc::span::Span::dummy(),
                )],
                catches: vec![CatchClause {
                    exception_types: vec!["Exception".into()],
                    variable: None,
                    body: vec![Stmt::echo(Expr::int_lit(1))],
                }],
                finally_body: None,
            },
            elephc::span::Span::dummy(),
        )]
    );
}

#[test]
fn test_parse_throw_expression_in_null_coalesce() {
    let stmts = parse_source("<?php $value = $maybe ?? throw new Exception();");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Assign { name, value } => {
            assert_eq!(name, "value");
            match &value.kind {
                ExprKind::NullCoalesce { value, default } => {
                    assert_eq!(value.kind, ExprKind::Variable("maybe".into()));
                    match &default.kind {
                        ExprKind::Throw(inner) => match &inner.kind {
                            ExprKind::NewObject { class_name, .. } => {
                                assert_eq!(class_name, "Exception");
                            }
                            other => panic!("expected throw new Exception(), got {:?}", other),
                        },
                        other => panic!("expected throw expression, got {:?}", other),
                    }
                }
                other => panic!("expected null coalesce, got {:?}", other),
            }
        }
        other => panic!("expected assignment, got {:?}", other),
    }
}
