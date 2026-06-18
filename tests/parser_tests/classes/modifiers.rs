//! Purpose:
//! Integration or regression tests for parser AST coverage of class modifiers, including abstract class with implements, readonly class flag, and final class flag.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Parses `abstract class` with `implements` interfaces and verifies the AST captures
/// `is_abstract`, interface names, and an abstract method with visibility and no body.
#[test]
fn test_parse_abstract_class_with_implements() {
    let stmts = parse_source(
        "<?php abstract class Base implements Named, Tagged { abstract protected function load(); }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            implements,
            is_abstract,
            methods,
            ..
        } => {
            assert_eq!(name, "Base");
            assert_eq!(implements, &vec!["Named".to_string(), "Tagged".to_string()]);
            assert!(*is_abstract);
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "load");
            assert!(methods[0].is_abstract);
            assert!(!methods[0].has_body);
            assert!(methods[0].body.is_empty());
        }
        _ => panic!("Expected ClassDecl"),
    }
}

/// Parses a `readonly class` declaration and verifies `is_readonly_class` is set and
/// the property name is preserved.
#[test]
fn test_parse_readonly_class_flag() {
    let stmts = parse_source("<?php readonly class User { public $id; }");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            is_readonly_class,
            properties,
            ..
        } => {
            assert_eq!(name, "User");
            assert!(*is_readonly_class);
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0].name, "id");
        }
        other => panic!("Expected readonly ClassDecl, got {:?}", other),
    }
}

/// Parses a `final class` declaration and verifies `is_final` is set while
/// `is_abstract` and `is_readonly_class` are both false.
#[test]
fn test_parse_final_class_flag() {
    let stmts = parse_source("<?php final class User { public function id() { return 1; } }");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            is_final,
            is_abstract,
            is_readonly_class,
            methods,
            ..
        } => {
            assert_eq!(name, "User");
            assert!(*is_final);
            assert!(!is_abstract);
            assert!(!is_readonly_class);
            assert_eq!(methods.len(), 1);
        }
        other => panic!("Expected final ClassDecl, got {:?}", other),
    }
}

/// Parses both `final readonly` and `readonly final` orderings and verifies both
/// flags are set in each case.
#[test]
fn test_parse_final_readonly_class_flags() {
    for source in [
        "<?php final readonly class User {}",
        "<?php readonly final class User {}",
    ] {
        let stmts = parse_source(source);
        match &stmts[0].kind {
            StmtKind::ClassDecl {
                name,
                is_final,
                is_readonly_class,
                ..
            } => {
                assert_eq!(name, "User");
                assert!(*is_final);
                assert!(*is_readonly_class);
            }
            other => panic!("Expected final readonly ClassDecl, got {:?}", other),
        }
    }
}

/// Parses `abstract readonly class` and verifies both `is_abstract` and
/// `is_readonly_class` are set.
#[test]
fn test_parse_abstract_readonly_class_flags() {
    let stmts = parse_source("<?php abstract readonly class User {}");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            is_abstract,
            is_readonly_class,
            ..
        } => {
            assert_eq!(name, "User");
            assert!(*is_abstract);
            assert!(*is_readonly_class);
        }
        other => panic!("Expected abstract readonly ClassDecl, got {:?}", other),
    }
}

/// Parses a method with the `final` modifier inside a non-final class and verifies
/// `is_final` is set, the method has a body, and is not abstract.
#[test]
fn test_parse_final_method_flag() {
    let stmts = parse_source("<?php class User { final public function id() { return 1; } }");
    match &stmts[0].kind {
        StmtKind::ClassDecl { methods, .. } => {
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "id");
            assert!(methods[0].is_final);
            assert!(!methods[0].is_abstract);
            assert!(methods[0].has_body);
        }
        other => panic!("Expected ClassDecl with final method, got {:?}", other),
    }
}

/// Parses a class with typed properties including nullable types, visibility, default
/// values, and the `final` property flag. Verifies name, type_expr, visibility, and
/// `is_final` are all correctly captured.
#[test]
fn test_parse_typed_properties() {
    let stmts = parse_source(
        "<?php class User { public int $id; protected ?string $email = null; final public string $name = \"Ada\"; }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl { properties, .. } => {
            assert_eq!(properties.len(), 3);
            assert_eq!(properties[0].name, "id");
            assert_eq!(properties[0].type_expr, Some(TypeExpr::Int));
            assert_eq!(properties[1].name, "email");
            assert_eq!(properties[1].visibility, Visibility::Protected);
            assert_eq!(
                properties[1].type_expr,
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Str)))
            );
            assert_eq!(properties[2].name, "name");
            assert_eq!(properties[2].type_expr, Some(TypeExpr::Str));
            assert!(properties[2].is_final);
        }
        other => panic!("Expected ClassDecl with typed properties, got {:?}", other),
    }
}

/// Parses a constructor with promoted parameters covering all visibility levels,
/// nullable and non-nullable types, default values, `readonly` promoted params,
/// and by-reference (`&`) promoted params. Verifies the promoted properties are
/// correctly added to the class and the original constructor body is preserved.
#[test]
fn test_parse_constructor_promoted_properties() {
    let stmts = parse_source(
        "<?php class User { public function __construct(public int $id, private string $name, readonly ?int $rank = null, protected int &$score) { echo $id; } }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            properties,
            methods,
            ..
        } => {
            assert_eq!(properties.len(), 4);
            assert_eq!(properties[0].name, "id");
            assert_eq!(properties[0].visibility, Visibility::Public);
            assert_eq!(properties[0].type_expr, Some(TypeExpr::Int));
            assert!(!properties[0].readonly);
            assert_eq!(properties[1].name, "name");
            assert_eq!(properties[1].visibility, Visibility::Private);
            assert_eq!(properties[1].type_expr, Some(TypeExpr::Str));
            assert_eq!(properties[2].name, "rank");
            assert_eq!(properties[2].visibility, Visibility::Public);
            assert_eq!(
                properties[2].type_expr,
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Int)))
            );
            assert!(properties[2].readonly);
            assert!(!properties[2].by_ref);
            assert_eq!(properties[3].name, "score");
            assert_eq!(properties[3].visibility, Visibility::Protected);
            assert_eq!(properties[3].type_expr, Some(TypeExpr::Int));
            assert!(properties[3].by_ref);

            assert_eq!(methods.len(), 1);
            let ctor = &methods[0];
            assert_eq!(ctor.name, "__construct");
            assert_eq!(ctor.params.len(), 4);
            assert_eq!(ctor.params[0].0, "id");
            assert_eq!(ctor.params[0].1, Some(TypeExpr::Int));
            assert_eq!(ctor.params[1].0, "name");
            assert_eq!(ctor.params[1].1, Some(TypeExpr::Str));
            assert_eq!(ctor.params[2].0, "rank");
            assert!(ctor.params[2].2.is_some());
            assert_eq!(ctor.params[3].0, "score");
            assert!(ctor.params[3].3);
            assert_eq!(ctor.body.len(), 5);
            assert_promoted_assignment(&ctor.body[0], "id");
            assert_promoted_assignment(&ctor.body[1], "name");
            assert_promoted_assignment(&ctor.body[2], "rank");
            assert_promoted_assignment(&ctor.body[3], "score");
            match &ctor.body[4].kind {
                StmtKind::Echo(expr) => assert_eq!(expr.kind, ExprKind::Variable("id".into())),
                other => panic!("Expected original constructor body after promotion, got {:?}", other),
            }
        }
        other => panic!("Expected ClassDecl with promoted properties, got {:?}", other),
    }
}

/// Parses an abstract property hook contract in an `abstract class` and verifies
/// `is_abstract` and both `hooks.get` and `hooks.set` are set.
#[test]
fn test_parse_abstract_property_hook_contract() {
    let stmts = parse_source(
        "<?php abstract class Box { abstract public int $value { get; set; } }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl { properties, .. } => {
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0].name, "value");
            assert!(properties[0].is_abstract);
            assert!(properties[0].hooks.get);
            assert!(properties[0].hooks.set);
        }
        other => panic!("Expected ClassDecl, got {:?}", other),
    }
}

/// Parses an `abstract` property with a by-reference getter hook inside a trait and
/// verifies `is_abstract` and `hooks.get_by_ref` are set.
#[test]
fn test_parse_trait_abstract_property_hook_contract() {
    let stmts = parse_source(
        "<?php trait NeedsValue { abstract public int $value { &get; } }",
    );
    match &stmts[0].kind {
        StmtKind::TraitDecl { properties, .. } => {
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0].name, "value");
            assert!(properties[0].is_abstract);
            assert!(properties[0].hooks.get_by_ref);
        }
        other => panic!("Expected TraitDecl, got {:?}", other),
    }
}

/// Verifies that `public private(set)` parses as read visibility public with an asymmetric
/// write (`set`) visibility of private.
#[test]
fn test_parse_asymmetric_visibility_public_private_set() {
    let stmts = parse_source("<?php class C { public private(set) int $v = 1; }");
    match &stmts[0].kind {
        StmtKind::ClassDecl { properties, .. } => {
            assert_eq!(properties[0].visibility, Visibility::Public);
            assert_eq!(properties[0].set_visibility, Some(Visibility::Private));
        }
        other => panic!("Expected ClassDecl, got {:?}", other),
    }
}

/// Verifies that a lone `private(set)` modifier leaves the read visibility at its public default
/// while setting the write visibility to private.
#[test]
fn test_parse_asymmetric_visibility_set_only_defaults_get_public() {
    let stmts = parse_source("<?php class C { private(set) string $name = \"x\"; }");
    match &stmts[0].kind {
        StmtKind::ClassDecl { properties, .. } => {
            assert_eq!(properties[0].visibility, Visibility::Public);
            assert_eq!(properties[0].set_visibility, Some(Visibility::Private));
        }
        other => panic!("Expected ClassDecl, got {:?}", other),
    }
}

/// Verifies that an ordinary property without a `(set)` modifier has no asymmetric write
/// visibility.
#[test]
fn test_parse_property_without_set_visibility() {
    let stmts = parse_source("<?php class C { public int $v = 1; }");
    match &stmts[0].kind {
        StmtKind::ClassDecl { properties, .. } => {
            assert_eq!(properties[0].set_visibility, None);
        }
        other => panic!("Expected ClassDecl, got {:?}", other),
    }
}

/// Verifies a concrete `get` hook with a short body records the hook flag and generates a synthetic
/// `__propget_<name>` accessor (and no setter) to carry the body through later passes.
#[test]
fn test_parse_concrete_get_hook_generates_accessor() {
    let stmts = parse_source("<?php class C { public int $x { get => 42; } }");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            properties, methods, ..
        } => {
            assert_eq!(properties[0].name, "x");
            assert!(properties[0].hooks.get);
            assert!(!properties[0].hooks.set);
            assert!(methods.iter().any(|m| m.name == "__propget_x"));
            assert!(!methods.iter().any(|m| m.name == "__propset_x"));
        }
        other => panic!("Expected ClassDecl, got {:?}", other),
    }
}

/// Verifies a `get`/`set` hook pair with block bodies generates both accessor methods, and that a
/// custom `set` parameter name is carried onto the generated setter's single parameter.
#[test]
fn test_parse_get_set_hooks_generate_both_accessors() {
    let stmts = parse_source(
        "<?php class C { private int $n = 0; public int $v { get { return $this->n; } set(int $newVal) { $this->n = $newVal; } } }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            properties, methods, ..
        } => {
            let v = properties.iter().find(|p| p.name == "v").unwrap();
            assert!(v.hooks.get);
            assert!(v.hooks.set);
            let getter = methods.iter().find(|m| m.name == "__propget_v").unwrap();
            assert!(getter.params.is_empty());
            let setter = methods.iter().find(|m| m.name == "__propset_v").unwrap();
            assert_eq!(setter.params.len(), 1);
            assert_eq!(setter.params[0].0, "newVal");
        }
        other => panic!("Expected ClassDecl, got {:?}", other),
    }
}

/// Verifies a short `set => expr;` hook generates a setter accessor whose body writes to
/// the hooked property's own raw backing slot.
#[test]
fn test_parse_short_set_hook_generates_backing_assignment() {
    let stmts = parse_source(
        "<?php class C { public string $v { get => $this->v; set => trim($value); } }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            properties, methods, ..
        } => {
            let v = properties.iter().find(|p| p.name == "v").unwrap();
            assert!(v.hooks.get);
            assert!(v.hooks.set);
            let setter = methods.iter().find(|m| m.name == "__propset_v").unwrap();
            assert_eq!(setter.params.len(), 1);
            match &setter.body[0].kind {
                StmtKind::PropertyAssign {
                    object,
                    property,
                    value,
                } => {
                    assert!(matches!(&object.kind, ExprKind::This));
                    assert_eq!(property, "v");
                    assert!(matches!(&value.kind, ExprKind::FunctionCall { .. }));
                }
                other => panic!("Expected synthetic PropertyAssign, got {:?}", other),
            }
        }
        other => panic!("Expected ClassDecl, got {:?}", other),
    }
}
