//! Purpose:
//! Integration or regression tests for parser AST coverage of magic constants, including dunder dir magic constant, dunder dir magic constant case insensitive, and dunder file magic constant.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

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
