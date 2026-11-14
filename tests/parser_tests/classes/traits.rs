//! Purpose:
//! Integration or regression tests for parser AST coverage of class traits, including trait decl and use adaptations, trait use as protected, and trait use insteadof.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Parses a `TraitDecl` node from `trait A { ... }` and a `ClassDecl` node with a trait use
/// that adapts `A::foo` to a private alias `bar`. Verifies AST shape: trait name, empty
/// trait_uses on the trait decl, single method `foo`, and the alias adaptation with private
/// visibility on the class's trait use.
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

/// Parses a class that uses a trait with `A::foo as protected` (visibility adaptation, no alias).
/// Verifies the `TraitAdaptation::Alias` variant has `alias = None` and `visibility = Protected`.
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

/// A semi-reserved keyword names the method in every adaptation position: the bare target,
/// the `Trait::method` target and the alias, while a modifier right after `as` stays a visibility
/// change and a modifier after a visibility is the alias (`as protected static`).
#[test]
fn test_parse_trait_adaptation_keyword_method_names() {
    let stmts = parse_source(
        "<?php trait A { public function match() {} public function list() {} } class Box { use A { match as protected matchContent; A::list as print; match as public; list as private static; } }",
    );
    let StmtKind::ClassDecl { trait_uses, .. } = &stmts[1].kind else {
        panic!("Expected ClassDecl");
    };
    let adaptations = &trait_uses[0].adaptations;
    assert_eq!(adaptations.len(), 4);
    let expected = [
        (None, "match", Some("matchContent"), Some(Visibility::Protected)),
        (Some("A"), "list", Some("print"), None),
        (None, "match", None, Some(Visibility::Public)),
        (None, "list", Some("static"), Some(Visibility::Private)),
    ];
    for (adaptation, (want_trait, want_method, want_alias, want_visibility)) in adaptations.iter().zip(expected) {
        let TraitAdaptation::Alias { trait_name, method, alias, visibility } = adaptation else {
            panic!("Expected trait alias adaptation, got {adaptation:?}");
        };
        assert_eq!(trait_name.as_deref(), want_trait);
        assert_eq!(method, want_method);
        assert_eq!(alias.as_deref(), want_alias);
        assert_eq!(*visibility, want_visibility);
    }
}

/// Parses a class using two traits `A, B` with an `A::foo insteadof B` conflict resolution.
/// Verifies the `TraitAdaptation::InsteadOf` variant with `trait_name = A`, `method = foo`,
/// and `instead_of = vec!["B"]`.
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

/// Parses an enum using a trait and verifies the enum stores the trait-use adaptation metadata.
#[test]
fn test_parse_enum_trait_use_adaptation() {
    let stmts = parse_source(
        "<?php trait HasLabel { public function label() { return $this->name; } } enum Suit { use HasLabel { HasLabel::label as text; } case Hearts; }",
    );
    match &stmts[1].kind {
        StmtKind::EnumDecl {
            trait_uses,
            cases,
            methods,
            ..
        } => {
            assert_eq!(trait_uses.len(), 1);
            assert_eq!(trait_uses[0].trait_names, vec!["HasLabel".to_string()]);
            assert_eq!(trait_uses[0].adaptations.len(), 1);
            assert_eq!(cases.len(), 1);
            assert!(methods.is_empty());
            match &trait_uses[0].adaptations[0] {
                TraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias,
                    visibility,
                } => {
                    assert_eq!(trait_name.as_deref(), Some("HasLabel"));
                    assert_eq!(method, "label");
                    assert_eq!(alias.as_deref(), Some("text"));
                    assert_eq!(*visibility, None);
                }
                _ => panic!("Expected trait alias adaptation"),
            }
        }
        _ => panic!("Expected EnumDecl"),
    }
}

/// Parses `echo __TRAIT__;` and asserts the expression is `ExprKind::MagicConstant(MagicConstant::Trait)`.
#[test]
fn test_parse_dunder_trait_magic_constant() {
    let stmts = parse_source("<?php echo __TRAIT__;");
    assert_eq!(echoed_expr(&stmts), &ExprKind::MagicConstant(MagicConstant::Trait));
}

// --- ::class magic constant ---
