//! Purpose:
//! Integration or regression tests for parser AST coverage of magic constants, including dunder dir magic constant, dunder dir magic constant case insensitive, and dunder file magic constant.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;
use std::path::Path;

#[test]
fn test_parse_dunder_dir_magic_constant() {
    let stmts = parse_source("<?php echo __DIR__;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::MagicConstant(MagicConstant::Dir));
}

#[test]
fn test_parse_dunder_dir_magic_constant_case_insensitive() {
    let stmts = parse_source("<?php echo __dir__;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::MagicConstant(MagicConstant::Dir));
}

#[test]
fn test_parse_dunder_file_magic_constant() {
    let stmts = parse_source("<?php echo __FILE__;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::MagicConstant(MagicConstant::File));
}

#[test]
fn test_parse_dunder_line_lowers_to_int_literal() {
    // __LINE__ is substituted at parse time using the span line.
    let stmts = parse_source("<?php echo __LINE__;");
    match echoed_expr(&stmts) {
        ExprKind::IntLiteral(n) => assert_eq!(*n, 1),
        other => panic!("expected IntLiteral, got {:?}", other),
    }
}

#[test]
fn test_parse_dunder_line_reports_correct_line_inside_multiline() {
    let stmts = parse_source("<?php\n\necho __LINE__;\n");
    match echoed_expr(&stmts) {
        ExprKind::IntLiteral(n) => assert_eq!(*n, 3),
        other => panic!("expected IntLiteral, got {:?}", other),
    }
}

#[test]
fn test_parse_dunder_class_magic_constant() {
    let stmts = parse_source("<?php echo __CLASS__;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::MagicConstant(MagicConstant::Class));
}

#[test]
fn test_parse_dunder_method_magic_constant() {
    let stmts = parse_source("<?php echo __METHOD__;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::MagicConstant(MagicConstant::Method));
}

#[test]
fn test_magic_constants_lower_inside_first_class_callable_receiver() {
    let stmts = parse_source("<?php echo 1 |> make(__FILE__)->m(...);");
    let lowered = elephc::magic_constants::substitute_file_and_scope_constants(
        stmts,
        Path::new("/tmp/elephc/main.php"),
    );
    let expr = match &lowered[0].kind {
        StmtKind::Echo(expr) => expr,
        other => panic!("expected Echo, got {:?}", other),
    };
    match &expr.kind {
        ExprKind::Pipe { callable, .. } => match &callable.kind {
            ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
                assert_eq!(method, "m");
                match &object.kind {
                    ExprKind::FunctionCall { name, args } => {
                        assert_eq!(name.as_str(), "make");
                        assert_eq!(
                            args[0].kind,
                            ExprKind::StringLiteral("/tmp/elephc/main.php".into())
                        );
                    }
                    other => panic!("expected FunctionCall receiver, got {:?}", other),
                }
            }
            other => panic!("expected method callable, got {:?}", other),
        },
        other => panic!("expected Pipe, got {:?}", other),
    }
}

#[test]
fn test_parse_class_class_named() {
    let stmts = parse_source("<?php echo MyClass::class;");
    match echoed_expr(&stmts) {
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Named(name),
        } => assert_eq!(name.as_str(), "MyClass"),
        other => panic!("expected ClassConstant Named, got {:?}", other),
    }
}

#[test]
fn test_parse_class_class_self() {
    let stmts = parse_source("<?php echo self::class;");
    assert_eq!(
        echoed_expr(&stmts),
        &ExprKind::ClassConstant {
            receiver: StaticReceiver::Self_,
        }
    );
}

#[test]
fn test_parse_class_class_static() {
    let stmts = parse_source("<?php echo static::class;");
    assert_eq!(
        echoed_expr(&stmts),
        &ExprKind::ClassConstant {
            receiver: StaticReceiver::Static,
        }
    );
}

#[test]
fn test_parse_class_class_parent() {
    let stmts = parse_source("<?php echo parent::class;");
    assert_eq!(
        echoed_expr(&stmts),
        &ExprKind::ClassConstant {
            receiver: StaticReceiver::Parent,
        }
    );
}

// --- new self() / new static() / new parent() ---
