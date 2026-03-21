use elephc::lexer::tokenize;
use elephc::parser::ast::{BinOp, Expr, Stmt};
use elephc::parser::parse;

fn parse_source(src: &str) -> Vec<Stmt> {
    let tokens = tokenize(src).unwrap();
    parse(&tokens).unwrap()
}

#[test]
fn test_echo_string_literal() {
    let stmts = parse_source("<?php echo \"hello\";");
    assert_eq!(stmts, vec![Stmt::Echo(Expr::StringLiteral("hello".into()))]);
}

#[test]
fn test_echo_integer() {
    let stmts = parse_source("<?php echo 42;");
    assert_eq!(stmts, vec![Stmt::Echo(Expr::IntLiteral(42))]);
}

#[test]
fn test_variable_assignment() {
    let stmts = parse_source("<?php $x = 10;");
    assert_eq!(
        stmts,
        vec![Stmt::Assign {
            name: "x".into(),
            value: Expr::IntLiteral(10),
        }]
    );
}

#[test]
fn test_echo_variable() {
    let stmts = parse_source("<?php $x = 5; echo $x;");
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[1], Stmt::Echo(Expr::Variable("x".into())));
}

#[test]
fn test_negative_integer() {
    let stmts = parse_source("<?php echo -7;");
    assert_eq!(
        stmts,
        vec![Stmt::Echo(Expr::Negate(Box::new(Expr::IntLiteral(7))))]
    );
}

#[test]
fn test_arithmetic_precedence() {
    // 2 + 3 * 4 should parse as 2 + (3 * 4)
    let stmts = parse_source("<?php echo 2 + 3 * 4;");
    let expected = Stmt::Echo(Expr::BinaryOp {
        left: Box::new(Expr::IntLiteral(2)),
        op: BinOp::Add,
        right: Box::new(Expr::BinaryOp {
            left: Box::new(Expr::IntLiteral(3)),
            op: BinOp::Mul,
            right: Box::new(Expr::IntLiteral(4)),
        }),
    });
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_parenthesized_expr() {
    let stmts = parse_source("<?php echo (2 + 3) * 4;");
    let expected = Stmt::Echo(Expr::BinaryOp {
        left: Box::new(Expr::BinaryOp {
            left: Box::new(Expr::IntLiteral(2)),
            op: BinOp::Add,
            right: Box::new(Expr::IntLiteral(3)),
        }),
        op: BinOp::Mul,
        right: Box::new(Expr::IntLiteral(4)),
    });
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_concat_operator() {
    let stmts = parse_source("<?php echo \"a\" . \"b\";");
    let expected = Stmt::Echo(Expr::BinaryOp {
        left: Box::new(Expr::StringLiteral("a".into())),
        op: BinOp::Concat,
        right: Box::new(Expr::StringLiteral("b".into())),
    });
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_multiple_statements() {
    let stmts = parse_source("<?php $a = 1; $b = 2; echo $a;");
    assert_eq!(stmts.len(), 3);
}

#[test]
fn test_missing_semicolon() {
    let tokens = tokenize("<?php echo \"hi\"").unwrap();
    assert!(parse(&tokens).is_err());
}
