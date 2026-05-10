//! Purpose:
//! Integration or regression tests for parser AST coverage of expression operators, including arithmetic precedence, concat operator, and comparison lower than arithmetic.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_arithmetic_precedence() {
    let stmts = parse_source("<?php echo 2 + 3 * 4;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(2),
        BinOp::Add,
        Expr::binop(Expr::int_lit(3), BinOp::Mul, Expr::int_lit(4)),
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
fn test_comparison_lower_than_arithmetic() {
    // 1 + 2 == 3 should parse as (1 + 2) == 3
    let stmts = parse_source("<?php echo 1 + 2 == 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Add, Expr::int_lit(2)),
        BinOp::Eq,
        Expr::int_lit(3),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_concat_higher_than_comparison() {
    // "x" . 1 < 2 should parse as ("x" . 1) < 2 — PHP precedence
    let stmts = parse_source("<?php echo \"x\" . 1 < 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::string_lit("x"), BinOp::Concat, Expr::int_lit(1)),
        BinOp::Lt,
        Expr::int_lit(2),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_modulo_same_as_multiply() {
    // 10 % 3 * 2 should parse as (10 % 3) * 2
    let stmts = parse_source("<?php echo 10 % 3 * 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(10), BinOp::Mod, Expr::int_lit(3)),
        BinOp::Mul,
        Expr::int_lit(2),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Control flow ---

#[test]
fn test_strict_equal_parses() {
    let stmts = parse_source("<?php echo 1 === 1;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(1),
        BinOp::StrictEq,
        Expr::int_lit(1),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_strict_not_equal_parses() {
    let stmts = parse_source("<?php echo 1 !== 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(1),
        BinOp::StrictNotEq,
        Expr::int_lit(2),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_strict_equal_same_precedence_as_loose() {
    // 1 + 2 === 3 should parse as (1 + 2) === 3
    let stmts = parse_source("<?php echo 1 + 2 === 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Add, Expr::int_lit(2)),
        BinOp::StrictEq,
        Expr::int_lit(3),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Include/Require ---

#[test]
fn test_pow_operator_parses() {
    let stmts = parse_source("<?php echo 2 ** 3;");
    let expected = Stmt::echo(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(3)));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_pow_right_associative_parse() {
    // 2 ** 3 ** 2 should parse as 2 ** (3 ** 2)
    let stmts = parse_source("<?php echo 2 ** 3 ** 2;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(2),
        BinOp::Pow,
        Expr::binop(Expr::int_lit(3), BinOp::Pow, Expr::int_lit(2)),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_pow_higher_than_mul_parse() {
    // 3 * 2 ** 3 should parse as 3 * (2 ** 3)
    let stmts = parse_source("<?php echo 3 * 2 ** 3;");
    let expected = Stmt::echo(Expr::binop(
        Expr::int_lit(3),
        BinOp::Mul,
        Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(3)),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Type casting ---

#[test]
fn test_bitwise_and_lower_than_equality() {
    // 1 == 1 & 0 should parse as (1 == 1) & 0 — PHP precedence
    let stmts = parse_source("<?php echo 1 == 1 & 0;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::Eq, Expr::int_lit(1)),
        BinOp::BitAnd,
        Expr::int_lit(0),
    ));
    assert_eq!(stmts, vec![expected]);
}

#[test]
fn test_shift_higher_than_comparison() {
    // 1 << 2 < 10 should parse as (1 << 2) < 10 — PHP precedence
    let stmts = parse_source("<?php echo 1 << 2 < 10;");
    let expected = Stmt::echo(Expr::binop(
        Expr::binop(Expr::int_lit(1), BinOp::ShiftLeft, Expr::int_lit(2)),
        BinOp::Lt,
        Expr::int_lit(10),
    ));
    assert_eq!(stmts, vec![expected]);
}

// --- Null coalescing precedence ---
