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
    assert_eq!(stmts, vec![Stmt::echo(Expr::string_lit("hello"))]);
}

#[test]
fn test_echo_integer() {
    let stmts = parse_source("<?php echo 42;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::int_lit(42))]);
}

#[test]
fn test_variable_assignment() {
    let stmts = parse_source("<?php $x = 10;");
    assert_eq!(stmts, vec![Stmt::assign("x", Expr::int_lit(10))]);
}

#[test]
fn test_echo_variable() {
    let stmts = parse_source("<?php $x = 5; echo $x;");
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[1], Stmt::echo(Expr::var("x")));
}

#[test]
fn test_negative_integer() {
    let stmts = parse_source("<?php echo -7;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::negate(Expr::int_lit(7)))]);
}

#[test]
fn test_arithmetic_precedence() {
    // 2 + 3 * 4 should parse as 2 + (3 * 4)
    let stmts = parse_source("<?php echo 2 + 3 * 4;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(2),
        BinOp::Add,
        Expr::binop(Expr::int_lit(3), BinOp::Mul, Expr::int_lit(4)),
    ));
    assert_eq!(stmts, vec![expected]);
}

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
fn test_concat_operator() {
    let stmts = parse_source("<?php echo \"a\" . \"b\";");
    let expected = Stmt::echo(Expr::binop(
        Expr::string_lit("a"),
        BinOp::Concat,
        Expr::string_lit("b"),
    ));
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

#[test]
fn test_left_associativity() {
    // 1 - 2 - 3 should parse as (1 - 2) - 3
    let stmts = parse_source("<?php echo 1 - 2 - 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Sub, Expr::int_lit(2)),
        BinOp::Sub,
        Expr::int_lit(3),
    ));
    assert_eq!(stmts, vec![expected]);
}
