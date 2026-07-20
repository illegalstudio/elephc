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

/// Verifies that `<?php echo __DIR__;` parses to an `Echo` of `MagicConstant::Dir`.
#[test]
fn test_parse_dunder_dir_magic_constant() {
    let stmts = parse_source("<?php echo __DIR__;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::MagicConstant(MagicConstant::Dir));
}

/// Verifies that `<?php echo __dir__;` (lowercase) also parses to `MagicConstant::Dir`.
/// PHP magic constants are case-insensitive.
#[test]
fn test_parse_dunder_dir_magic_constant_case_insensitive() {
    let stmts = parse_source("<?php echo __dir__;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::MagicConstant(MagicConstant::Dir));
}

/// Verifies that `<?php echo __FILE__;` parses to an `Echo` of `MagicConstant::File`.
#[test]
fn test_parse_dunder_file_magic_constant() {
    let stmts = parse_source("<?php echo __FILE__;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::MagicConstant(MagicConstant::File));
}

/// Verifies that `__LINE__` is substituted at parse time to an integer literal using the span line.
/// `<?php echo __LINE__;` on line 1 must lower to `IntLiteral(1)`.
#[test]
fn test_parse_dunder_line_lowers_to_int_literal() {
    // __LINE__ is substituted at parse time using the span line.
    let stmts = parse_source("<?php echo __LINE__;");
    match echoed_expr(&stmts) {
        ExprKind::IntLiteral(n) => assert_eq!(*n, 1),
        other => panic!("expected IntLiteral, got {:?}", other),
    }
}

/// Verifies that `__LINE__` reports the correct line when the constant appears on a different
/// line than the opening tag. Parsing `<?php\n\necho __LINE__;\n` (line 3) must lower to `IntLiteral(3)`.
#[test]
fn test_parse_dunder_line_reports_correct_line_inside_multiline() {
    let stmts = parse_source("<?php\n\necho __LINE__;\n");
    match echoed_expr(&stmts) {
        ExprKind::IntLiteral(n) => assert_eq!(*n, 3),
        other => panic!("expected IntLiteral, got {:?}", other),
    }
}

/// Verifies that `<?php echo __CLASS__;` parses to an `Echo` of `MagicConstant::Class`.
#[test]
fn test_parse_dunder_class_magic_constant() {
    let stmts = parse_source("<?php echo __CLASS__;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::MagicConstant(MagicConstant::Class));
}

/// Verifies that `<?php echo __METHOD__;` parses to an `Echo` of `MagicConstant::Method`.
#[test]
fn test_parse_dunder_method_magic_constant() {
    let stmts = parse_source("<?php echo __METHOD__;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::MagicConstant(MagicConstant::Method));
}

/// Verifies that magic constants are correctly substituted inside first-class callable
/// receiver expressions (e.g., `make(__FILE__)->m(...)`). The `__FILE__` argument must lower
/// to a `StringLiteral` before the pipe operator processes the callable.
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

/// Verifies that `<?php echo MyClass::class;` parses to a `ClassConstant` with a `StaticReceiver::Named("MyClass")`.
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

/// Verifies that `<?php echo self::class;` parses to a `ClassConstant` with `StaticReceiver::Self_`.
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

/// Verifies that `<?php echo static::class;` parses to a `ClassConstant` with `StaticReceiver::Static`.
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

/// Verifies that `<?php echo parent::class;` parses to a `ClassConstant` with `StaticReceiver::Parent`.
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

/// Verifies that `$object::class` keeps the object expression as a dedicated runtime class-name receiver.
#[test]
fn test_parse_object_class_name() {
    let stmts = parse_source("<?php echo $pippo::class;");
    match echoed_expr(&stmts) {
        ExprKind::ObjectClassName { object } => {
            assert_eq!(object.kind, ExprKind::Variable("pippo".to_string()));
        }
        other => panic!("expected ObjectClassName, got {:?}", other),
    }
}

/// Verifies that `::class` accepts a call expression receiver without duplicating it in the AST.
#[test]
fn test_parse_call_expression_object_class_name() {
    let stmts = parse_source("<?php echo make()::class;");
    match echoed_expr(&stmts) {
        ExprKind::ObjectClassName { object } => match &object.kind {
            ExprKind::FunctionCall { name, args } => {
                assert_eq!(name.as_str(), "make");
                assert!(args.is_empty());
            }
            other => panic!("expected FunctionCall receiver, got {:?}", other),
        },
        other => panic!("expected ObjectClassName, got {:?}", other),
    }
}

// --- new self() / new static() / new parent() ---
