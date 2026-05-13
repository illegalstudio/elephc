//! Purpose:
//! Parser regression tests for PHP attribute syntax and AST persistence.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Declaration attributes are preserved where the AST carries metadata.
//! - Parameter and closure attributes are accepted for syntax parity.

use super::*;
use elephc::parser::ast::{AttributeGroup, ClassMethod, ClassProperty};

/// Helper: extract the first ClassDecl from a parsed program.
fn first_class_decl_name(stmts: &[Stmt]) -> &str {
    for stmt in stmts {
        if let StmtKind::ClassDecl { name, .. } = &stmt.kind {
            return name;
        }
    }
    panic!("expected a ClassDecl in {:?}", stmts);
}

fn class_decl<'a>(stmts: &'a [Stmt]) -> (&'a Vec<AttributeGroup>, &'a Vec<ClassProperty>, &'a Vec<ClassMethod>) {
    for stmt in stmts {
        if let StmtKind::ClassDecl { properties, methods, .. } = &stmt.kind {
            return (&stmt.attributes, properties, methods);
        }
    }
    panic!("expected a ClassDecl in {:?}", stmts);
}

#[test]
fn test_class_attribute_is_accepted_and_does_not_alter_decl() {
    // The class with an attribute parses to the same AST shape as the
    // bare class — attributes are consumed at parse time but not preserved
    // in the AST for v1.
    let with_attr = parse_source("<?php #[Foo] class C {}");
    let without = parse_source("<?php class C {}");
    assert_eq!(with_attr, without);
}

#[test]
fn test_method_attribute_is_accepted() {
    // `#[Required]` on a method must parse without error.
    // Persistence is verified by test_method_attribute_is_persisted below.
    let _ = parse_source(
        "<?php class Service { #[Required] public function setX(int $x): void {} }",
    );
}

#[test]
fn test_property_attribute_is_accepted() {
    // `#[Bar]` on a property must parse without error.
    // Persistence is verified by test_property_attribute_is_persisted below.
    let _ = parse_source("<?php class C { #[Bar] public int $n = 0; }");
}

#[test]
fn test_multiple_attributes_in_one_group() {
    // `#[A, B(1)]` should parse the same as no attributes.
    let with_attr = parse_source("<?php #[A, B(1, \"two\")] class D {}");
    assert_eq!(first_class_decl_name(&with_attr), "D");
}

#[test]
fn test_stacked_attribute_groups() {
    // Stacked groups `#[A] #[B]` are equivalent to `#[A, B]`.
    let stacked = parse_source("<?php #[A] #[B] class E {}");
    let combined = parse_source("<?php #[A, B] class E {}");
    let bare = parse_source("<?php class E {}");
    assert_eq!(stacked, combined);
    assert_eq!(stacked, bare);
}

#[test]
fn test_attribute_on_interface_method() {
    // `#[Pure]` on an interface method must parse without error. Member-level
    // persistence is verified by test_method_attribute_is_persisted.
    let _ = parse_source(
        "<?php interface I { #[Pure] public function f(): int; }",
    );
}

#[test]
fn test_attribute_on_function_decl() {
    let with_attr = parse_source("<?php #[Memoized] function f(): int { return 1; }");
    let without = parse_source("<?php function f(): int { return 1; }");
    assert_eq!(with_attr, without);
}

#[test]
fn test_attribute_on_enum_case() {
    // `#[Primary]` on an enum case must parse without error.
    // Persistence is verified by test_attribute_on_enum_case_is_persisted.
    let _ = parse_source(
        "<?php enum Color: int { #[Primary] case Red = 1; case Blue = 2; }",
    );
}

#[test]
fn test_qualified_attribute_name_parses() {
    // Fully-qualified names with leading and inner backslashes must be
    // accepted by the attribute parser.
    let stmts = parse_source(
        "<?php #[\\Symfony\\Contracts\\Service\\Attribute\\Required] class C {}",
    );
    assert_eq!(first_class_decl_name(&stmts), "C");
}

#[test]
fn test_attribute_on_function_parameter() {
    // PHP 8 allows attributes immediately before each parameter.
    let with_attr = parse_source(
        "<?php function f(#[Sensitive] string $s): void {}",
    );
    let without = parse_source("<?php function f(string $s): void {}");
    assert_eq!(with_attr, without);
}

#[test]
fn test_attribute_on_method_parameter() {
    let with_attr = parse_source(
        "<?php class S { public function call(#[Sensitive] string $s): void {} }",
    );
    let without = parse_source(
        "<?php class S { public function call(string $s): void {} }",
    );
    assert_eq!(with_attr, without);
}

#[test]
fn test_attribute_on_promoted_constructor_property() {
    // Attribute precedes the visibility modifier of a promoted property.
    let with_attr = parse_source(
        "<?php class S { public function __construct(#[Inject] public Logger $l) {} }",
    );
    let without = parse_source(
        "<?php class S { public function __construct(public Logger $l) {} }",
    );
    assert_eq!(with_attr, without);
}

#[test]
fn test_attribute_on_closure_expression() {
    let with_attr = parse_source(
        "<?php $f = #[Pure] function (int $x): int { return $x + 1; };",
    );
    let without = parse_source(
        "<?php $f = function (int $x): int { return $x + 1; };",
    );
    assert_eq!(with_attr, without);
}

#[test]
fn test_attribute_on_arrow_function() {
    let with_attr = parse_source("<?php $f = #[Pure] fn ($x) => $x + 1;");
    let without = parse_source("<?php $f = fn ($x) => $x + 1;");
    assert_eq!(with_attr, without);
}

#[test]
fn test_attribute_on_static_closure() {
    let with_attr = parse_source(
        "<?php $f = #[Pure] static function (int $x): int { return $x; };",
    );
    let without = parse_source(
        "<?php $f = static function (int $x): int { return $x; };",
    );
    assert_eq!(with_attr, without);
}

#[test]
fn test_attribute_on_static_arrow_function() {
    let with_attr = parse_source("<?php $f = #[Pure] static fn ($x) => $x;");
    let without = parse_source("<?php $f = static fn ($x) => $x;");
    assert_eq!(with_attr, without);
}

#[test]
fn test_attribute_on_closure_parameter() {
    let with_attr = parse_source(
        "<?php $f = function (#[Sensitive] string $s): void { };",
    );
    let without = parse_source(
        "<?php $f = function (string $s): void { };",
    );
    assert_eq!(with_attr, without);
}

#[test]
fn test_attribute_on_arrow_function_parameter() {
    let with_attr = parse_source("<?php $f = fn (#[X] int $a) => $a + 1;");
    let without = parse_source("<?php $f = fn (int $a) => $a + 1;");
    assert_eq!(with_attr, without);
}

#[test]
fn test_stacked_attributes_on_parameter() {
    let with_attr = parse_source(
        "<?php function f(#[A] #[B] int $x): void {}",
    );
    let without = parse_source("<?php function f(int $x): void {}");
    assert_eq!(with_attr, without);
}

// --- Persistence: attributes are now captured in the AST ---

#[test]
fn test_class_attribute_is_persisted_on_stmt() {
    let stmts = parse_source("<?php #[Foo] class C {}");
    let (groups, _props, _methods) = class_decl(&stmts);
    assert_eq!(groups.len(), 1, "one attribute group expected");
    assert_eq!(groups[0].attributes.len(), 1);
    assert_eq!(groups[0].attributes[0].name.as_str(), "Foo");
    assert!(groups[0].attributes[0].args.is_empty());
}

#[test]
fn test_attribute_args_are_captured() {
    let stmts = parse_source("<?php #[Bar(1, \"two\")] class C {}");
    let (groups, _, _) = class_decl(&stmts);
    let arg_count = groups[0].attributes[0].args.len();
    assert_eq!(arg_count, 2, "expected 2 args, got {}", arg_count);
}

#[test]
fn test_method_attribute_is_persisted() {
    let stmts = parse_source(
        "<?php class S { #[Required] public function setX(int $x): void {} }",
    );
    let (_, _props, methods) = class_decl(&stmts);
    let method = methods.iter().find(|m| m.name == "setX").expect("setX method");
    assert_eq!(method.attributes.len(), 1);
    assert_eq!(method.attributes[0].attributes[0].name.as_str(), "Required");
}

#[test]
fn test_property_attribute_is_persisted() {
    let stmts = parse_source(
        "<?php class C { #[Slot] public int $n = 0; }",
    );
    let (_, props, _) = class_decl(&stmts);
    let prop = props.iter().find(|p| p.name == "n").expect("n property");
    assert_eq!(prop.attributes.len(), 1);
    assert_eq!(prop.attributes[0].attributes[0].name.as_str(), "Slot");
}

#[test]
fn test_qualified_attribute_name_preserves_parts() {
    let stmts = parse_source(
        "<?php #[\\Symfony\\Contracts\\Service\\Attribute\\Required] class C {}",
    );
    let (groups, _, _) = class_decl(&stmts);
    let name = &groups[0].attributes[0].name;
    assert!(name.is_fully_qualified(), "expected fully-qualified name");
    assert_eq!(
        name.as_str(),
        "Symfony\\Contracts\\Service\\Attribute\\Required",
    );
}

#[test]
fn test_attribute_on_non_declaration_is_rejected() {
    // PHP rejects attributes on non-declaration statements like `echo`.
    assert!(parse_fails("<?php #[Foo] echo 1;"));
    assert!(parse_fails("<?php #[Foo] $x = 1;"));
    assert!(parse_fails("<?php #[Foo] if (true) {}"));
}

#[test]
fn test_attribute_on_enum_case_is_persisted() {
    let stmts = parse_source(
        "<?php enum Color: int { #[Primary] case Red = 1; case Blue = 2; }",
    );
    let cases = match &stmts[0].kind {
        StmtKind::EnumDecl { cases, .. } => cases,
        other => panic!("expected EnumDecl, got {:?}", other),
    };
    let red = cases.iter().find(|c| c.name == "Red").expect("Red case");
    let blue = cases.iter().find(|c| c.name == "Blue").expect("Blue case");
    assert_eq!(red.attributes.len(), 1);
    assert_eq!(red.attributes[0].attributes[0].name.as_str(), "Primary");
    assert!(blue.attributes.is_empty(), "Blue should have no attributes");
}
