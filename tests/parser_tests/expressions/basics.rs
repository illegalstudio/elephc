use super::*;

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

#[test]
fn test_negative_integer() {
    let stmts = parse_source("<?php echo -7;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::negate(Expr::int_lit(7)))]);
}

// --- Operator precedence ---

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

#[test]
fn test_return_value_parses() {
    let stmts = parse_source("<?php function f() { return 42; }");
    if let StmtKind::FunctionDecl { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Return(Some(_))));
    }
}

#[test]
fn test_return_void_parses() {
    let stmts = parse_source("<?php function f() { return; }");
    if let StmtKind::FunctionDecl { body, .. } = &stmts[0].kind {
        assert!(matches!(&body[0].kind, StmtKind::Return(None)));
    }
}

#[test]
fn test_cast_int_parses() {
    let stmts = parse_source("<?php echo (int)3.14;");
    assert_eq!(stmts.len(), 1);
}

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

#[test]
fn test_cast_not_confused_with_parens() {
    // (1 + 2) should NOT be parsed as a cast
    let stmts = parse_source("<?php echo (1 + 2);");
    assert_eq!(stmts.len(), 1);
}

// --- Float ---

#[test]
fn test_float_literal() {
    let stmts = parse_source("<?php echo 3.14;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::float_lit(3.14))]);
}

#[test]
fn test_negative_float() {
    let stmts = parse_source("<?php echo -3.14;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::negate(Expr::float_lit(3.14)))]);
}

// --- Associative arrays ---

#[test]
fn test_parse_nullable_shorthand_cannot_be_combined_with_union() {
    assert!(parse_fails("<?php ?int|string $value = null;"));
}

// --- Magic constants ---
