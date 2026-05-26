//! Purpose:
//! Integration or regression tests for parser AST coverage of statements, including echo string literal, echo integer, and variable assignment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets cover successful AST shapes plus malformed syntax that must fail during parsing.

use super::*;

#[test]
// Verifies that `<?php echo "hello";` parses to a single `Echo` stmt containing a `StringLiteral`.
fn test_echo_string_literal() {
    let stmts = parse_source("<?php echo \"hello\";");
    assert_eq!(stmts, vec![Stmt::echo(Expr::string_lit("hello"))]);
}

#[test]
// Verifies that `<?php echo 42;` parses to a single `Echo` stmt containing an `IntLiteral(42)`.
fn test_echo_integer() {
    let stmts = parse_source("<?php echo 42;");
    assert_eq!(stmts, vec![Stmt::echo(Expr::int_lit(42))]);
}

#[test]
// Verifies that `<?php $x = 10;` parses to a simple `Assign` stmt with variable name "x"
// and integer literal value 10.
fn test_variable_assignment() {
    let stmts = parse_source("<?php $x = 10;");
    assert_eq!(stmts, vec![Stmt::assign("x", Expr::int_lit(10))]);
}

#[test]
// Verifies that `<?php $x = 5; echo $x;` parses to two stmts: assign and echo.
// Asserts the echoed expression is a `Variable("x")`.
fn test_echo_variable() {
    let stmts = parse_source("<?php $x = 5; echo $x;");
    assert_eq!(stmts.len(), 2);
    assert_eq!(stmts[1], Stmt::echo(Expr::var("x")));
}

// --- Unary ---

#[test]
// Verifies that `<?php $a = 1; $b = 2; echo $a;` parses to three stmts in order.
fn test_multiple_statements() {
    let stmts = parse_source("<?php $a = 1; $b = 2; echo $a;");
    assert_eq!(stmts.len(), 3);
}

// --- Parse errors ---

#[test]
// Verifies that `<?php echo "hi"` (missing semicolon) fails during parsing.
fn test_missing_semicolon() {
    assert!(parse_fails("<?php echo \"hi\""));
}

#[test]
// Verifies that `<?php if (1) { echo "a";` (missing closing brace) fails during parsing.
fn test_missing_closing_brace() {
    assert!(parse_fails("<?php if (1) { echo \"a\";"));
}

#[test]
// Verifies that `<?php if 1 { echo "a"; }` (missing parentheses around condition) fails parsing.
fn test_missing_condition_parens() {
    assert!(parse_fails("<?php if 1 { echo \"a\"; }"));
}

#[test]
// Verifies that `<?php print "hello";` parses as an `ExprStmt` wrapping `Expr::print(...)`.
// PHP's `print` is an expression construct (returns 1), distinct from `echo`.
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
