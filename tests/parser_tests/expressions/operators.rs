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
// Verifies that `<?php echo 2 + 3 * 4;` parses as `2 + (3 * 4)` — multiplication has higher
// precedence than addition, matching PHP's arithmetic precedence.
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
// Verifies that `<?php echo "a" . "b";` parses as a binary concat operation.
// The `.` operator concatenates two string literals.
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
// Verifies that `<?php echo 1 + 2 == 3;` parses as `(1 + 2) == 3` — addition has higher
// precedence than equality, matching PHP's precedence rules.
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
// Verifies that `<?php echo "x" . 1 < 2;` parses as `("x" . 1) < 2` — concatenation has higher
// precedence than comparison, matching PHP precedence.
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
// Verifies that `<?php echo 10 % 3 * 2;` parses as `(10 % 3) * 2` — modulo and multiplication
// have the same precedence and are left-associative.
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
// Verifies that `<?php echo 1 === 1;` parses as a strict equality binary operation.
// The `===` operator checks type-strict equality in PHP.
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
// Verifies that `<?php echo 1 !== 2;` parses as a strict inequality binary operation.
// The `!==` operator checks type-strict inequality in PHP.
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
// Verifies that `<?php echo 1 + 2 === 3;` parses as `(1 + 2) === 3` — arithmetic has higher
// precedence than strict equality, consistent with PHP's precedence table.
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
// Verifies that `<?php echo 2 ** 3;` parses as an exponentiation binary operation.
// The `**` operator computes the power of left operand raised to the right operand.
fn test_pow_operator_parses() {
    let stmts = parse_source("<?php echo 2 ** 3;");
    let expected = Stmt::echo(Expr::binop(Expr::int_lit(2), BinOp::Pow, Expr::int_lit(3)));
    assert_eq!(stmts, vec![expected]);
}

#[test]
// Verifies that `<?php echo 2 ** 3 ** 2;` parses as `2 ** (3 ** 2)` — exponentiation is
// right-associative in PHP, so the rightmost `**` groups first.
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
// Verifies that `<?php echo 3 * 2 ** 3;` parses as `3 * (2 ** 3)` — exponentiation has
// higher precedence than multiplication, matching PHP precedence rules.
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
// Verifies that `<?php echo 1 == 1 & 0;` parses as `(1 == 1) & 0` — bitwise AND has lower
// precedence than loose equality, matching PHP precedence table.
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
// Verifies that `<?php echo 1 << 2 < 10;` parses as `(1 << 2) < 10` — shift operators have
// higher precedence than comparison, matching PHP precedence.
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

// --- Unary operators ---

#[test]
// Verifies that `<?php echo ++$i;`, `<?php echo $i++;`, `<?php echo --$i;`, and
// `<?php echo $i--;` parse correctly as pre/post increment/decrement expressions.
// Tests all four variants to ensure the parser distinguishes prefix vs postfix forms.
fn test_increment_decrement_parses() {
    let pre_inc = parse_source("<?php echo ++$i;");
    assert_eq!(echoed_expr(&pre_inc), &ExprKind::PreIncrement("i".into()));
    let post_inc = parse_source("<?php echo $i++;");
    assert_eq!(echoed_expr(&post_inc), &ExprKind::PostIncrement("i".into()));
    let pre_dec = parse_source("<?php echo --$i;");
    assert_eq!(echoed_expr(&pre_dec), &ExprKind::PreDecrement("i".into()));
    let post_dec = parse_source("<?php echo $i--;");
    assert_eq!(echoed_expr(&post_dec), &ExprKind::PostDecrement("i".into()));
}

#[test]
// Verifies that `<?php echo ~$x;` parses as a bitwise NOT unary operation.
// The `~` operator inverts bits of its operand.
fn test_bitwise_not_parses() {
    let stmts = parse_source("<?php echo ~$x;");
    assert_eq!(
        echoed_expr(&stmts),
        &ExprKind::BitNot(Box::new(Expr::var("x")))
    );
}

#[test]
// Verifies that `<?php echo $arr[0]();` parses as an ExprCall node whose callee is an
// ArrayAccess. This exercises callable expressions where the callee is a subscript result.
fn test_expr_call_parses() {
    // `$arr[0]()` calls the result of an array access — an ExprCall node.
    let stmts = parse_source("<?php echo $arr[0]();");
    match echoed_expr(&stmts) {
        ExprKind::ExprCall { callee, args } => {
            assert!(matches!(callee.kind, ExprKind::ArrayAccess { .. }));
            assert!(args.is_empty());
        }
        other => panic!("expected ExprCall, got {:?}", other),
    }
}

// --- Null coalescing precedence ---
