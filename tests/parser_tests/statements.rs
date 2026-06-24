//! Purpose:
//! Integration or regression tests for parser AST coverage of statements, including echo string literal, echo integer, and variable assignment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets cover successful AST shapes plus malformed syntax that must fail during parsing.

use super::*;

/// Verifies that `<?php echo "hello";` parses to a single `Echo` stmt containing a `StringLiteral`.
#[test]
fn test_echo_string_literal() {
    let stmts = parse_source("<?php echo \"hello\";");
    assert_eq!(stmts, vec![Stmt::echo(Expr::string_lit("hello"))]);
}

/// Verifies that `<?php echo 42;` parses to a single `Echo` stmt containing an `IntLiteral(42)`.
#[test]
fn test_echo_integer() {
    let stmts = parse_source("<?php echo 42;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::int_lit(42))]);
}

/// Verifies that `<?php $x = 10;` parses to a simple `Assign` stmt with variable name "x"
/// and integer literal value 10.
#[test]
fn test_variable_assignment() {
    let stmts = parse_source("<?php $x = 10;");
    assert_eq!(stmts, vec![Stmt::assign("x", Expr::int_lit(10))]);
}

/// Verifies that `<?php $x = 5; echo $x;` parses to two stmts: assign and echo.
/// Asserts the echoed expression is a `Variable("x")`.
#[test]
fn test_echo_variable() {
    let stmts = parse_source("<?php $x = 5; echo $x;");
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[1], Stmt::echo(Expr::var("x")));
}

// --- Unary ---

/// Verifies that `<?php $a = 1; $b = 2; echo $a;` parses to three stmts in order.
#[test]
fn test_multiple_statements() {
    let stmts = parse_source("<?php $a = 1; $b = 2; echo $a;");
    assert_eq!(stmts.len(), 3);
}

// --- Parse errors ---

/// Verifies that `<?php echo "hi"` (missing semicolon) fails during parsing.
#[test]
fn test_missing_semicolon() {
    assert!(parse_fails("<?php echo \"hi\""));
}

/// Verifies that `<?php if (1) { echo "a";` (missing closing brace) fails during parsing.
#[test]
fn test_missing_closing_brace() {
    assert!(parse_fails("<?php if (1) { echo \"a\";"));
}

/// Verifies that `<?php if 1 { echo "a"; }` (missing parentheses around condition) fails parsing.
#[test]
fn test_missing_condition_parens() {
    assert!(parse_fails("<?php if 1 { echo \"a\"; }"));
}

/// Verifies that `<?php print "hello";` parses as an `ExprStmt` wrapping `Expr::print(...)`.
/// PHP's `print` is an expression construct (returns 1), distinct from `echo`.
#[test]
fn test_print_parses_as_expression_statement() {
    let stmts = parse_source("<?php print \"hello\";");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::ExprStmt(Expr::print(Expr::string_lit("hello"))),
            elephc::span::Span::dummy(),
        )]
    );
}

/// Verifies parenthesized expressions are accepted as standalone expression statements.
#[test]
fn test_parenthesized_expression_statement() {
    let stmts = parse_source("<?php (1 + 2);");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::ExprStmt(Expr::binop(Expr::int_lit(1), BinOp::Add, Expr::int_lit(2))),
            elephc::span::Span::dummy(),
        )]
    );
}

/// Verifies a statement led by a literal value (not a variable or keyword) parses as a bare
/// expression statement. `0 > $T;` routes through the dispatcher's prefix-expression fallback
/// to `ExprStmt(0 > $T)` instead of erroring at statement position.
#[test]
fn test_value_led_expression_statement() {
    let stmts = parse_source("<?php 0 > $T;");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::ExprStmt(Expr::binop(Expr::int_lit(0), BinOp::Gt, Expr::var("T"))),
            elephc::span::Span::dummy(),
        )]
    );
}

/// Verifies the short-circuit `cond && action;` idiom parses as a single expression statement
/// whose top-level operator is `&&`, with the literal-led comparison on the left. This is the
/// Symfony intl-normalizer shape (`0 > $T && $T += 0x40;`).
#[test]
fn test_short_circuit_action_statement_parses() {
    let stmts = parse_source("<?php 0 < $x && $x;");
    assert_eq!(
        stmts,
        vec![Stmt::new(
            StmtKind::ExprStmt(Expr::binop(
                Expr::binop(Expr::int_lit(0), BinOp::Lt, Expr::var("x")),
                BinOp::And,
                Expr::var("x"),
            )),
            elephc::span::Span::dummy(),
        )]
    );
}

/// Verifies a bare `new C();` (object construction with no assignment) parses as an expression
/// statement rather than erroring at statement position.
#[test]
fn test_bare_new_object_statement_parses() {
    let stmts = parse_source("<?php new C();");
    assert_eq!(stmts.len(), 1);
    assert!(matches!(stmts[0].kind, StmtKind::ExprStmt(_)));
}

/// Verifies a token that cannot begin an expression (`=>`) is still rejected at statement
/// position, confirming the bare-expression fallback is gated on prefix-expression starters.
#[test]
fn test_non_expression_token_at_statement_position_fails() {
    assert!(parse_fails("<?php => 5;"));
}
