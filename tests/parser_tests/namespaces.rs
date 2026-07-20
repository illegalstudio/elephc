//! Purpose:
//! Integration or regression tests for parser AST coverage of namespaces, including namespace semicolon and use group, namespace block with qualified names, and dunder namespace magic constant.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Parses a namespace declaration with semicolon syntax and a use group import
/// that combines class, function, and const imports with aliases.
#[test]
fn test_parse_namespace_semicolon_and_use_group() {
    let stmts = parse_source(
        "<?php namespace App\\Core; use Lib\\Utils\\{Formatter, function render as draw, const ANSWER};",
    );
    assert_eq!(stmts.len(), 2);
    match &stmts[0].kind {
        StmtKind::NamespaceDecl { name } => {
            assert_eq!(name.as_ref().map(Name::as_str), Some("App\\Core"));
        }
        other => panic!("expected namespace decl, got {:?}", other),
    }
    match &stmts[1].kind {
        StmtKind::UseDecl { imports } => {
            assert_eq!(imports.len(), 3);
            assert_eq!(imports[0].kind, UseKind::Class);
            assert_eq!(imports[0].name.as_str(), "Lib\\Utils\\Formatter");
            assert_eq!(imports[0].alias, "Formatter");
            assert_eq!(imports[1].kind, UseKind::Function);
            assert_eq!(imports[1].name.as_str(), "Lib\\Utils\\render");
            assert_eq!(imports[1].alias, "draw");
            assert_eq!(imports[2].kind, UseKind::Const);
            assert_eq!(imports[2].name.as_str(), "Lib\\Utils\\ANSWER");
            assert_eq!(imports[2].alias, "ANSWER");
        }
        other => panic!("expected use decl, got {:?}", other),
    }
}

/// Parses lexer-tokenized predefined constants in every grouped-use suffix position.
#[test]
fn test_parse_grouped_use_const_tokenized_names() {
    let stmts = parse_source(
        "<?php namespace App; use const Vendor\\{PHP_INT_MAX as MAX, PHP_INT_MIN as MIN};",
    );
    match &stmts[1].kind {
        StmtKind::UseDecl { imports } => {
            assert_eq!(imports.len(), 2);
            assert_eq!(imports[0].name.as_str(), "Vendor\\PHP_INT_MAX");
            assert_eq!(imports[0].alias, "MAX");
            assert_eq!(imports[1].name.as_str(), "Vendor\\PHP_INT_MIN");
            assert_eq!(imports[1].alias, "MIN");
        }
        other => panic!("expected use decl, got {:?}", other),
    }
}

/// Parses a namespace block containing a class with extends, implements, trait use,
/// and a fully-qualified static method call inside a method body.
#[test]
fn test_parse_namespace_block_with_qualified_names() {
    let stmts = parse_source(
        "<?php namespace App\\Models { class User extends Base\\Record implements \\Contracts\\Jsonable { use Shared\\Loggable; public function make() { return Factory\\UserFactory::build(); } } }",
    );
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::NamespaceBlock { name, body } => {
            assert_eq!(name.as_ref().map(Name::as_str), Some("App\\Models"));
            assert_eq!(body.len(), 1);
            match &body[0].kind {
                StmtKind::ClassDecl {
                    extends,
                    implements,
                    trait_uses,
                    methods,
                    ..
                } => {
                    assert_eq!(extends.as_ref().map(Name::as_str), Some("Base\\Record"));
                    assert_eq!(implements.len(), 1);
                    assert!(implements[0].is_fully_qualified());
                    assert_eq!(implements[0].as_str(), "Contracts\\Jsonable");
                    assert_eq!(trait_uses[0].trait_names[0].as_str(), "Shared\\Loggable");
                    match &methods[0].body[0].kind {
                        StmtKind::Return(Some(expr)) => match &expr.kind {
                            ExprKind::StaticMethodCall {
                                receiver, method, ..
                            } => {
                                match receiver {
                                    StaticReceiver::Named(name) => {
                                        assert_eq!(name.as_str(), "Factory\\UserFactory");
                                    }
                                    other => panic!("expected named receiver, got {:?}", other),
                                }
                                assert_eq!(method, "build");
                            }
                            other => panic!("expected static method call, got {:?}", other),
                        },
                        other => panic!("expected return stmt, got {:?}", other),
                    }
                }
                other => panic!("expected class decl, got {:?}", other),
            }
        }
        other => panic!("expected namespace block, got {:?}", other),
    }
}

/// Parses the `__NAMESPACE__` magic constant in an echo statement and verifies
/// it is lowered to the internal `MagicConstant::Namespace` variant.
#[test]
fn test_parse_dunder_namespace_magic_constant() {
    let stmts = parse_source("<?php echo __NAMESPACE__;");
    assert_eq!(
        echoed_expr(&stmts),
        &ExprKind::MagicConstant(MagicConstant::Namespace)
    );
}
