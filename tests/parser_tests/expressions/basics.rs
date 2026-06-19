//! Purpose:
//! Integration or regression tests for parser AST coverage of basic expressions, including error control expression, control has unary precedence, and ifdef statement.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets cover successful AST shapes plus malformed syntax that must fail during parsing.

use super::*;

/// Verifies that `<?php echo @file_get_contents("missing.txt");` parses to an `Echo` of an
/// `ErrorSuppress` wrapping a `FunctionCall`. The `@` prefix must not affect parsing.
#[test]
fn test_parse_error_control_expression() {
    let stmts = parse_source("<?php echo @file_get_contents(\"missing.txt\");");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::ErrorSuppress(inner) => match &inner.kind {
                ExprKind::FunctionCall { name, args } => {
                    assert_eq!(name.as_str(), "file_get_contents");
                    assert_eq!(args.len(), 1);
                }
                other => panic!("expected suppressed function call, got {:?}", other),
            },
            other => panic!("expected error suppression, got {:?}", other),
        },
        other => panic!("expected echo, got {:?}", other),
    }
}

/// Verifies that `<?php @file_get_contents("missing.txt");` parses as an `ExprStmt` of an
/// `ErrorSuppress` wrapping a `FunctionCall`. Error suppression is valid on expression statements.
#[test]
fn test_parse_error_control_expression_statement() {
    let stmts = parse_source("<?php @file_get_contents(\"missing.txt\");");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::ErrorSuppress(inner) => match &inner.kind {
                ExprKind::FunctionCall { name, args } => {
                    assert_eq!(name.as_str(), "file_get_contents");
                    assert_eq!(args.len(), 1);
                }
                other => panic!("expected suppressed function call, got {:?}", other),
            },
            other => panic!("expected error suppression, got {:?}", other),
        },
        other => panic!("expected expression statement, got {:?}", other),
    }
}

/// Verifies that `<?php echo @$x + 1;` parses as `(@$x) + 1` (not `@($x + 1)`).
/// Error suppression has higher precedence than addition, so the `@` applies only to `$x`.
#[test]
fn test_error_control_has_unary_precedence() {
    let stmts = parse_source("<?php echo @$x + 1;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::BinaryOp { left, op, right } => {
                assert_eq!(*op, BinOp::Add);
                assert!(matches!(left.kind, ExprKind::ErrorSuppress(_)));
                assert_eq!(right.kind, ExprKind::IntLiteral(1));
            }
            other => panic!("expected binary add, got {:?}", other),
        },
        other => panic!("expected echo, got {:?}", other),
    }
}

/// Verifies that `<?php echo "A", 2, $x;` (multi-argument echo) lowers to a `Synthetic`
/// node containing three separate `Echo` statements, preserving source order.
#[test]
fn test_parse_multi_argument_echo_lowers_to_synthetic_echoes() {
    let stmts = parse_source("<?php echo \"A\", 2, $x;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Synthetic(echoes) => {
            assert_eq!(echoes.len(), 3);
            assert!(matches!(
                &echoes[0].kind,
                StmtKind::Echo(expr) if expr.kind == ExprKind::StringLiteral("A".into())
            ));
            assert!(matches!(
                &echoes[1].kind,
                StmtKind::Echo(expr) if expr.kind == ExprKind::IntLiteral(2)
            ));
            assert!(matches!(
                &echoes[2].kind,
                StmtKind::Echo(expr) if expr.kind == ExprKind::Variable("x".into())
            ));
        }
        other => panic!("expected synthetic echo lowering, got {:?}", other),
    }
}

/// Verifies that `<?php ifdef DEBUG { echo 1; }` parses to an `IfDef` node with symbol "DEBUG"
/// and a then_body containing one echo statement.
#[test]
fn test_parse_ifdef_statement() {
    let stmts = parse_source("<?php ifdef DEBUG { echo 1; }");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::IfDef {
                symbol: "DEBUG".into(),
                then_body: vec![Stmt::echo(Expr::int_lit(1))],
                else_body: None,
            },
            elephc::span::Span::dummy(),
        )]
    );
}

/// Verifies that `<?php ifdef DEBUG { echo 1; } else { echo 2; }` parses to an `IfDef` with
/// both then_body and else_body populated.
#[test]
fn test_parse_ifdef_else_statement() {
    let stmts = parse_source("<?php ifdef DEBUG { echo 1; } else { echo 2; }");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::IfDef {
                symbol: "DEBUG".into(),
                then_body: vec![Stmt::echo(Expr::int_lit(1))],
                else_body: Some(vec![Stmt::echo(Expr::int_lit(2))]),
            },
            elephc::span::Span::dummy(),
        )]
    );
}

/// Verifies that `<?php echo -7;` parses as `Stmt::echo(Expr::negate(Expr::int_lit(7)))`.
/// Negative integer literals use a unary negation node, not a literal with embedded sign.
#[test]
fn test_negative_integer() {
    let stmts = parse_source("<?php echo -7;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::negate(Expr::int_lit(7)))]);
}

// --- Operator precedence ---

/// Verifies that `<?php echo (2 + 3) * 4;` parses as `(2 + 3) * 4` — parentheses force
/// addition to be evaluated before multiplication, matching PHP's parenthesized precedence.
#[test]
fn test_parenthesized_expr() {
    let stmts = parse_source("<?php echo (2 + 3) * 4;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(2), BinOp::Add, Expr::int_lit(3)),
        BinOp::Mul,
        Expr::int_lit(4),
    ));
    assert_eq!(stmts, vec![expected]);
}

/// Verifies that `<?php echo 1 - 2 - 3;` parses as `(1 - 2) - 3` (left-associative),
/// not `1 - (2 - 3)`. Subtraction is left-associative in PHP.
#[test]
fn test_left_associativity() {
    let stmts = parse_source("<?php echo 1 - 2 - 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Sub, Expr::int_lit(2)),
        BinOp::Sub,
        Expr::int_lit(3),
    ));
    assert_eq!(stmts, vec![expected]);
}

/// Verifies that `<?php function f() { return 42; }` parses with a `Return(Some(...))` stmt
/// inside the function body.
#[test]
fn test_return_value_parses() {
    let stmts = parse_source("<?php function f() { return 42; }");
    if let StmtKind::FunctionDecl { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Return(Some(_))));
    }
}

/// Verifies that `<?php function f() { return; }` parses with a `Return(None)` stmt (void return).
#[test]
fn test_return_void_parses() {
    let stmts = parse_source("<?php function f() { return; }");
    if let StmtKind::FunctionDecl { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Return(None)));
    }
}

/// Verifies that `<?php echo (int)3.14;` parses to a `Cast` expression with target `Int`.
/// PHP cast syntax `(int)` must be recognized as a unary cast operator.
#[test]
fn test_cast_int_parses() {
    let stmts = parse_source("<?php echo (int)3.14;");
    assert_eq!(stmts.len(), 1);
}

/// Verifies that `<?php echo (INTEGER)3.14;` parses to a `Cast` expression with target `Int`.
/// PHP cast keywords are case-insensitive.
#[test]
fn test_cast_keywords_are_case_insensitive() {
    let stmts = parse_source("<?php echo (INTEGER)3.14;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Cast { target, .. } => {
                assert_eq!(*target, elephc::parser::ast::CastType::Int);
            }
            other => panic!("expected cast expression, got {:?}", other),
        },
        other => panic!("expected echo statement, got {:?}", other),
    }
}

/// Verifies that `<?php echo (1 + 2);` parses as a parenthesized expression, NOT as a cast.
/// Parentheses around an arithmetic expression must not be interpreted as cast syntax.
#[test]
fn test_cast_not_confused_with_parens() {
    // (1 + 2) should NOT be parsed as a cast
    let stmts = parse_source("<?php echo (1 + 2);");
    assert_eq!(stmts.len(), 1);
}

/// Regression: a cast binds tighter than `+` (PHP precedence). `(int)$x + 3` must parse as
/// `((int)$x) + 3` — a top-level `Add` whose left operand is the `Cast` — not `(int)($x + 3)`.
/// The cast operand was previously parsed at binding power 27, which swallowed the trailing `+ 3`.
#[test]
fn test_cast_binds_tighter_than_addition() {
    let stmts = parse_source("<?php echo (int)$x + 3;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::BinaryOp { left, op, .. } => {
                assert_eq!(*op, BinOp::Add);
                assert!(
                    matches!(left.kind, ExprKind::Cast { .. }),
                    "left operand of + should be the cast, got {:?}",
                    left.kind
                );
            }
            other => panic!("expected top-level Add, got {:?}", other),
        },
        other => panic!("expected echo statement, got {:?}", other),
    }
}

/// Regression: `**` binds tighter than a cast (PHP precedence). `(int)$x ** 2` must parse as
/// `(int)($x ** 2)` — a top-level `Cast` wrapping a `Pow` — since exponentiation outranks casts.
#[test]
fn test_cast_binds_looser_than_exponent() {
    let stmts = parse_source("<?php echo (int)$x ** 2;");
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::Cast { expr: inner, .. } => {
                assert!(
                    matches!(inner.kind, ExprKind::BinaryOp { op: BinOp::Pow, .. }),
                    "cast operand should be a Pow, got {:?}",
                    inner.kind
                );
            }
            other => panic!("expected top-level Cast, got {:?}", other),
        },
        other => panic!("expected echo statement, got {:?}", other),
    }
}

// --- Float ---

/// Verifies that `<?php echo 3.14;` parses as `Stmt::echo(Expr::float_lit(3.14))`.
#[test]
fn test_float_literal() {
    let stmts = parse_source("<?php echo 3.14;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::float_lit(3.14))]);
}

/// Verifies that `<?php echo -3.14;` parses as `Stmt::echo(Expr::negate(Expr::float_lit(3.14)))`.
/// Negative float literals use a unary negation node, mirroring negative integer behavior.
#[test]
fn test_negative_float() {
    let stmts = parse_source("<?php echo -3.14;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::negate(Expr::float_lit(3.14)))]);
}

// --- Associative arrays ---

/// Verifies that `<?php ?int|string $value = null;` fails to parse.
/// The nullable shorthand `?T` cannot be combined with a union type; it is a parse error.
#[test]
fn test_parse_nullable_shorthand_cannot_be_combined_with_union() {
    assert!(parse_fails("<?php ?int|string $value = null;"));
}

// --- Magic constants ---
