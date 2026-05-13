//! Purpose:
//! Integration or regression tests for parser AST coverage of class traits, including trait decl and use adaptations, trait use as protected, and trait use insteadof.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

#[test]
fn test_parse_trait_decl_and_use_adaptations() {
    let stmts = parse_source(
        "<?php trait A { public function foo() { return 1; } } class Box { use A { A::foo as private bar; } }",
    );
    match &stmts[0].kind {
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
            ..
        } => {
            assert_eq!(name, "A");
            assert!(trait_uses.is_empty());
            assert!(properties.is_empty());
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "foo");
        }
        _ => panic!("Expected TraitDecl"),
    }
    match &stmts[1].kind {
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            trait_uses,
            properties,
            methods,
            ..
        } => {
            assert_eq!(name, "Box");
            assert_eq!(extends, &None);
            assert!(implements.is_empty());
            assert!(!is_abstract);
            assert!(properties.is_empty());
            assert!(methods.is_empty());
            assert_eq!(trait_uses.len(), 1);
            assert_eq!(trait_uses[0].trait_names, vec!["A".to_string()]);
            assert_eq!(trait_uses[0].adaptations.len(), 1);
            match &trait_uses[0].adaptations[0] {
                TraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias,
                    visibility,
                } => {
                    assert_eq!(trait_name.as_deref(), Some("A"));
                    assert_eq!(method, "foo");
                    assert_eq!(alias.as_deref(), Some("bar"));
                    assert_eq!(*visibility, Some(Visibility::Private));
                }
                _ => panic!("Expected trait alias adaptation"),
            }
        }
        _ => panic!("Expected ClassDecl"),
    }
}

#[test]
fn test_parse_trait_use_as_protected() {
    let stmts = parse_source(
        "<?php trait A { public function foo() { return 1; } } class Box { use A { A::foo as protected; } }",
    );
    match &stmts[1].kind {
        StmtKind::ClassDecl { trait_uses, .. } => {
            assert_eq!(trait_uses.len(), 1);
            assert_eq!(trait_uses[0].adaptations.len(), 1);
            match &trait_uses[0].adaptations[0] {
                TraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias,
                    visibility,
                } => {
                    assert_eq!(trait_name.as_deref(), Some("A"));
                    assert_eq!(method, "foo");
                    assert_eq!(alias, &None);
                    assert_eq!(*visibility, Some(Visibility::Protected));
                }
                _ => panic!("Expected trait alias adaptation"),
            }
        }
        _ => panic!("Expected ClassDecl"),
    }
}

#[test]
fn test_parse_trait_use_insteadof() {
    let stmts = parse_source(
        "<?php trait A { public function foo() { return 1; } } trait B { public function foo() { return 2; } } class Box { use A, B { A::foo insteadof B; } }",
    );
    match &stmts[2].kind {
        StmtKind::ClassDecl { trait_uses, .. } => {
            assert_eq!(trait_uses.len(), 1);
            assert_eq!(trait_uses[0].adaptations.len(), 1);
            match &trait_uses[0].adaptations[0] {
                TraitAdaptation::InsteadOf {
                    trait_name,
                    method,
                    instead_of,
                } => {
                    assert_eq!(trait_name.as_deref(), Some("A"));
                    assert_eq!(method, "foo");
                    assert_eq!(instead_of, &vec!["B".to_string()]);
                }
                _ => panic!("Expected trait insteadof adaptation"),
            }
        }
        _ => panic!("Expected ClassDecl"),
    }
}

#[test]
fn test_parse_dunder_trait_magic_constant() {
    let stmts = parse_source("<?php echo __TRAIT__;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::MagicConstant(MagicConstant::Trait));
}

// --- ::class magic constant ---
