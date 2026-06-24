//! Purpose:
//! Groups the expression parsing integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for basics, operators, modern PHP operators, assignments, arrays, string offsets, and match expressions.

use super::*;

#[path = "expressions/basics.rs"]
mod basics;
#[path = "expressions/operators.rs"]
mod operators;
#[path = "expressions/modern_ops/mod.rs"]
mod modern_ops;
#[path = "expressions/assignments.rs"]
mod assignments;
#[path = "expressions/arrays_match.rs"]
mod arrays_match;

/// Verifies that `$obj->$method()` desugars to `call_user_func([$obj, $method])`, reusing the
/// runtime dynamic-dispatch path.
#[test]
fn test_dynamic_method_call_desugars_to_call_user_func() {
    let stmts = parse_source("<?php $obj->$m(7);");
    let expr = match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => expr,
        other => panic!("Expected ExprStmt, got {:?}", other),
    };
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            assert_eq!(name.as_str(), "call_user_func");
            // First arg is the [$obj, $m] callable array, then the forwarded 7.
            assert!(matches!(&args[0].kind, ExprKind::ArrayLiteral(items) if items.len() == 2));
            assert_eq!(args.len(), 2);
            assert!(matches!(args[1].kind, ExprKind::IntLiteral(7)));
        }
        other => panic!("Expected call_user_func FunctionCall, got {:?}", other),
    }
}

/// Verifies that `$cls::method()` with a dynamic class receiver also desugars to call_user_func.
#[test]
fn test_dynamic_static_call_desugars_to_call_user_func() {
    let stmts = parse_source("<?php $cls::build();");
    let expr = match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => expr,
        other => panic!("Expected ExprStmt, got {:?}", other),
    };
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            assert_eq!(name.as_str(), "call_user_func");
            assert!(matches!(&args[0].kind, ExprKind::ArrayLiteral(items) if items.len() == 2));
        }
        other => panic!("Expected call_user_func FunctionCall, got {:?}", other),
    }
}

/// Verifies a fully-qualified reference to a predefined global constant (`\PHP_INT_MAX`) parses
/// to the same lowered literal as the unqualified form: the leading `\` (global namespace) is
/// skipped because these constants are lexed as dedicated tokens, not identifiers.
#[test]
fn test_fq_predefined_constant_parses_as_literal() {
    let stmts = parse_source("<?php echo \\PHP_INT_MAX;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::IntLiteral(i64::MAX));
}

/// Verifies the leading-backslash skip is scoped to predefined constants: `\` before a token that
/// is neither an identifier nor a global constant (here an integer literal) is still a name error.
#[test]
fn test_backslash_before_non_name_still_fails() {
    assert!(parse_fails("<?php echo \\123;"));
}
