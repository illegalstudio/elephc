//! Purpose:
//! Parser coverage for variable variables (`$$name`, `$$$name`, `${expr}`) and their postfixes.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A variable variable parses into the frame-access shape, an `ArrayAccess` whose base is the
//!   unforgeable `elephc::variable_variables::FRAME_LOCAL` pseudo-variable.

use super::*;
use elephc::variable_variables::FRAME_LOCAL;

/// Returns the name expression of a frame access, panicking on any other shape.
fn frame_name(expr: &Expr) -> &Expr {
    match &expr.kind {
        ExprKind::ArrayAccess { array, index } => {
            assert!(matches!(&array.kind, ExprKind::Variable(n) if n == FRAME_LOCAL));
            index
        }
        other => panic!("expected a frame access, got {other:?}"),
    }
}

/// Returns the echoed expression of the first statement.
fn echoed(src: &str) -> Expr {
    match &parse_source(src)[0].kind {
        StmtKind::Echo(expr) => expr.clone(),
        other => panic!("expected echo, got {other:?}"),
    }
}

/// `$$a` names the variable whose name is the value of `$a`.
#[test]
fn test_variable_variable_parses_as_frame_access() {
    let expr = echoed("<?php echo $$a;");
    assert!(matches!(&frame_name(&expr).kind, ExprKind::Variable(n) if n == "a"));
}

/// `$$a[0]` is `($$a)[0]` (PHP 7+ left-to-right), not `${$a[0]}`.
#[test]
fn test_variable_variable_dimension_binds_after_name() {
    let expr = echoed("<?php echo $$a[0];");
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        panic!("expected a dimension, got {:?}", expr.kind);
    };
    assert!(matches!(index.kind, ExprKind::IntLiteral(0)));
    assert!(matches!(&frame_name(array).kind, ExprKind::Variable(n) if n == "a"));
}

/// `$$$a` nests, and `${'x' . $y}` takes an arbitrary braced expression as the name.
#[test]
fn test_variable_variable_nested_and_braced_names() {
    let nested = echoed("<?php echo $$$a;");
    assert!(matches!(&frame_name(frame_name(&nested)).kind, ExprKind::Variable(n) if n == "a"));
    let braced = echoed("<?php echo ${'x' . $y};");
    assert!(matches!(&frame_name(&braced).kind, ExprKind::BinaryOp { .. }));
}

/// `$$a = 1;` starts an expression statement holding an assignment to the frame access.
#[test]
fn test_variable_variable_assignment_statement() {
    let stmts = parse_source("<?php $$a = 1;");
    let StmtKind::ExprStmt(expr) = &stmts[0].kind else {
        panic!("expected an expression statement, got {:?}", stmts[0].kind);
    };
    let ExprKind::Assignment { target, .. } = &expr.kind else {
        panic!("expected an assignment, got {:?}", expr.kind);
    };
    frame_name(target);
}

/// An unterminated `${` reports the missing brace.
#[test]
fn test_variable_variable_unclosed_brace_errors() {
    let tokens = elephc::lexer::tokenize("<?php echo ${'a';").unwrap();
    let err = elephc::parser::parse(&tokens).unwrap_err();
    assert!(err.message.contains("Expected '}'"), "{}", err.message);
}
