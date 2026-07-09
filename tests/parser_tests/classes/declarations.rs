//! Purpose:
//! Integration or regression tests for parser AST coverage of class declarations, including class decl, new object, and class decl with extends.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Verifies that `<?php class Point { public $x; private $y = 1; ... }` parses to a `ClassDecl`
/// with two properties (public and private), two methods (one static), and no extends/implements.
#[test]
fn test_parse_class_decl() {
    let stmts = parse_source("<?php class Point { public $x; private $y = 1; public function get() { return $this->x; } public static function origin() { return new Point(); } }");
    match &stmts[0].kind {
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
            assert_eq!(name, "Point");
            assert_eq!(extends, &None);
            assert!(implements.is_empty());
            assert!(!is_abstract);
            assert!(trait_uses.is_empty());
            assert_eq!(properties.len(), 2);
            assert_eq!(properties[0].name, "x");
            assert_eq!(properties[0].visibility, Visibility::Public);
            assert_eq!(properties[1].name, "y");
            assert_eq!(properties[1].visibility, Visibility::Private);
            assert!(properties[1].default.is_some());
            assert_eq!(methods.len(), 2);
            assert_eq!(methods[0].name, "get");
            assert!(!methods[0].is_static);
            assert_eq!(methods[1].name, "origin");
            assert!(methods[1].is_static);
        }
        _ => panic!("Expected ClassDecl"),
    }
}

/// Verifies that `clone`, a PHP operator keyword, is still accepted as a method name.
#[test]
fn test_parse_clone_named_method() {
    let stmts =
        parse_source("<?php class Image { public function clone(): Image { return $this; } }");
    match &stmts[0].kind {
        StmtKind::ClassDecl { methods, .. } => {
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "clone");
        }
        _ => panic!("Expected ClassDecl"),
    }
}

/// Verifies that `<?php $p = new Point(1, 2);` parses `new` expression with constructor args
/// into `StmtKind::Assign` wrapping `ExprKind::NewObject` with class name and argument list.
#[test]
fn test_parse_new_object() {
    let stmts = parse_source("<?php $p = new Point(1, 2);");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::NewObject { class_name, args } => {
                assert_eq!(class_name, "Point");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected NewObject"),
        },
        _ => panic!("Expected Assign"),
    }
}

/// Verifies that `<?php $p = new Point;` parses `new` without constructor parentheses
/// into `ExprKind::NewObject` with empty args, matching PHP's optional-parens rule.
#[test]
fn test_parse_new_object_no_parens() {
    let stmts = parse_source("<?php $p = new Point;");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::NewObject { class_name, args } => {
                assert_eq!(class_name, "Point");
                assert!(args.is_empty());
            }
            _ => panic!("Expected NewObject"),
        },
        _ => panic!("Expected Assign"),
    }
}

/// Verifies that `<?php $o = new $cls(1, 2);` parses dynamic instantiation into
/// `ExprKind::NewDynamic` whose `name_expr` is the class-name variable, with the
/// constructor argument list captured.
#[test]
fn test_parse_new_dynamic_object() {
    let stmts = parse_source("<?php $o = new $cls(1, 2);");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::NewDynamic { name_expr, args } => {
                assert_eq!(name_expr.kind, ExprKind::Variable("cls".to_string()));
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected NewDynamic"),
        },
        _ => panic!("Expected Assign"),
    }
}

/// Verifies that `<?php new Point(1, 2);` parses as an expression statement.
#[test]
fn test_parse_new_object_expression_statement() {
    let stmts = parse_source("<?php new Point(1, 2);");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::NewObject { class_name, args } => {
                assert_eq!(class_name, "Point");
                assert_eq!(args.len(), 2);
            }
            other => panic!("Expected NewObject, got {:?}", other),
        },
        other => panic!("Expected ExprStmt, got {:?}", other),
    }
}

/// Verifies that `<?php new $className();` parses as a dynamic-new expression statement.
#[test]
fn test_parse_new_dynamic_expression_statement() {
    let stmts = parse_source("<?php new $className();");
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::NewDynamic { name_expr, args } => {
                assert!(matches!(&name_expr.kind, ExprKind::Variable(name) if name == "className"));
                assert!(args.is_empty());
            }
            other => panic!("Expected NewDynamic, got {:?}", other),
        },
        other => panic!("Expected ExprStmt, got {:?}", other),
    }
}

/// Verifies that `<?php class Child extends Base { ... }` parses to `ClassDecl` with the
/// extends name set and the subclass body (method count, name) correctly captured.
#[test]
fn test_parse_class_decl_with_extends() {
    let stmts =
        parse_source("<?php class Child extends Base { public function run() { return 1; } }");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            name,
            extends,
            methods,
            ..
        } => {
            assert_eq!(name, "Child");
            assert_eq!(extends.as_deref(), Some("Base"));
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "run");
        }
        _ => panic!("Expected ClassDecl"),
    }
}

/// Verifies that class methods preserve `&...$items` as by-reference variadic metadata.
#[test]
fn test_parse_method_by_ref_variadic_param() {
    let stmts = parse_source("<?php class Box { public function collect(&...$items) {} }");
    match &stmts[0].kind {
        StmtKind::ClassDecl { methods, .. } => {
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "collect");
            assert_eq!(methods[0].variadic.as_deref(), Some("items"));
            assert!(methods[0].variadic_by_ref);
        }
        _ => panic!("Expected ClassDecl"),
    }
}

/// Verifies that `<?php interface Named extends Renderable, Jsonable { public function name(); }`
/// parses to `InterfaceDecl` with multiple extends names, one abstract method (no body),
/// and confirms `is_abstract` and `has_body` flags are set correctly.
#[test]
fn test_parse_interface_decl() {
    let stmts = parse_source(
        "<?php interface Named extends Renderable, Jsonable { public function name(); }",
    );
    match &stmts[0].kind {
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
            ..
        } => {
            assert_eq!(name, "Named");
            assert_eq!(
                extends,
                &vec!["Renderable".to_string(), "Jsonable".to_string()]
            );
            assert_eq!(methods.len(), 1);
            assert_eq!(methods[0].name, "name");
            assert!(methods[0].is_abstract);
            assert!(!methods[0].has_body);
            assert!(methods[0].body.is_empty());
        }
        _ => panic!("Expected InterfaceDecl"),
    }
}

/// Verifies that `<?php interface HasName { public string $name { get; set; } }` parses an
/// interface property with explicit getter/setter hooks into `InterfaceDecl` with the hooks
/// flags set on the property entry.
#[test]
fn test_parse_interface_property_hooks() {
    let stmts = parse_source(
        "<?php interface HasName { public string $name { get; set; } }",
    );
    match &stmts[0].kind {
        StmtKind::InterfaceDecl {
            name,
            properties,
            ..
        } => {
            assert_eq!(name, "HasName");
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0].name, "name");
            assert!(properties[0].is_abstract);
            assert!(properties[0].hooks.get);
            assert!(properties[0].hooks.set);
        }
        other => panic!("Expected InterfaceDecl, got {:?}", other),
    }
}

/// Verifies that `<?php echo new self();` parses `new self` into `ExprKind::NewScopedObject`
/// with `StaticReceiver::Self_` and no constructor arguments.
#[test]
fn test_parse_new_self() {
    let stmts = parse_source("<?php echo new self();");
    match echoed_expr(&stmts) {
        ExprKind::NewScopedObject {
            receiver: StaticReceiver::Self_,
            args,
        } => assert!(args.is_empty()),
        other => panic!("expected NewScopedObject Self_, got {:?}", other),
    }
}

/// Verifies that `<?php echo new self;` parses `new self` without parentheses into
/// `ExprKind::NewScopedObject` with `StaticReceiver::Self_` and empty args.
#[test]
fn test_parse_new_self_no_parens() {
    let stmts = parse_source("<?php echo new self;");
    match echoed_expr(&stmts) {
        ExprKind::NewScopedObject {
            receiver: StaticReceiver::Self_,
            args,
        } => assert!(args.is_empty()),
        other => panic!("expected NewScopedObject Self_, got {:?}", other),
    }
}

/// Verifies that `<?php echo new static();` parses `new static` into `ExprKind::NewScopedObject`
/// with `StaticReceiver::Static` and no constructor arguments.
#[test]
fn test_parse_new_static() {
    let stmts = parse_source("<?php echo new static();");
    match echoed_expr(&stmts) {
        ExprKind::NewScopedObject {
            receiver: StaticReceiver::Static,
            args,
        } => assert!(args.is_empty()),
        other => panic!("expected NewScopedObject Static, got {:?}", other),
    }
}

/// Verifies that `<?php echo new static;` parses `new static` without parentheses into
/// `ExprKind::NewScopedObject` with `StaticReceiver::Static` and empty args.
#[test]
fn test_parse_new_static_no_parens() {
    let stmts = parse_source("<?php echo new static;");
    match echoed_expr(&stmts) {
        ExprKind::NewScopedObject {
            receiver: StaticReceiver::Static,
            args,
        } => assert!(args.is_empty()),
        other => panic!("expected NewScopedObject Static, got {:?}", other),
    }
}

/// Verifies that `<?php echo new parent(1, 2);` parses `new parent` with positional args into
/// `ExprKind::NewScopedObject` with `StaticReceiver::Parent` and two constructor arguments.
#[test]
fn test_parse_new_parent_with_args() {
    let stmts = parse_source("<?php echo new parent(1, 2);");
    match echoed_expr(&stmts) {
        ExprKind::NewScopedObject {
            receiver: StaticReceiver::Parent,
            args,
        } => assert_eq!(args.len(), 2),
        other => panic!("expected NewScopedObject Parent, got {:?}", other),
    }
}

/// Verifies that `<?php echo new parent;` parses `new parent` without parentheses into
/// `ExprKind::NewScopedObject` with `StaticReceiver::Parent` and empty args.
#[test]
fn test_parse_new_parent_no_parens() {
    let stmts = parse_source("<?php echo new parent;");
    match echoed_expr(&stmts) {
        ExprKind::NewScopedObject {
            receiver: StaticReceiver::Parent,
            args,
        } => assert!(args.is_empty()),
        other => panic!("expected NewScopedObject Parent, got {:?}", other),
    }
}

/// Verifies that `<?php echo new $cls;` parses `new $cls` without parentheses into
/// `ExprKind::NewDynamic` with empty args.
#[test]
fn test_parse_new_dynamic_no_parens() {
    let stmts = parse_source("<?php echo new $cls;");
    match echoed_expr(&stmts) {
        ExprKind::NewDynamic { args, .. } => assert!(args.is_empty()),
        other => panic!("expected NewDynamic, got {:?}", other),
    }
}

// --- Static closures ---

/// Verifies that `self`, `static`, and `parent` parse as named type expressions in method
/// return position, kept symbolic for the checker to resolve to the enclosing class later.
#[test]
fn test_parse_relative_class_return_types() {
    let stmts = parse_source(
        "<?php class C { public function a(): self {} public static function b(): static {} public function c(): parent {} }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl { methods, .. } => {
            assert_eq!(
                methods[0].return_type,
                Some(TypeExpr::Named(Name::unqualified("self")))
            );
            assert_eq!(
                methods[1].return_type,
                Some(TypeExpr::Named(Name::unqualified("static")))
            );
            assert_eq!(
                methods[2].return_type,
                Some(TypeExpr::Named(Name::unqualified("parent")))
            );
        }
        _ => panic!("Expected ClassDecl"),
    }
}

/// Verifies that `self` parses in parameter and (nullable) property type positions.
#[test]
fn test_parse_relative_class_param_and_property_types() {
    let stmts = parse_source(
        "<?php class C { public ?self $next = null; public function link(self $other): void {} }",
    );
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            properties,
            methods,
            ..
        } => {
            assert_eq!(
                properties[0].type_expr,
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Named(
                    Name::unqualified("self")
                ))))
            );
            assert_eq!(
                methods[0].params[0].1,
                Some(TypeExpr::Named(Name::unqualified("self")))
            );
        }
        _ => panic!("Expected ClassDecl"),
    }
}

/// Verifies that `new class { ... }` is rewritten to `new <synthetic>()` and that the class body
/// is hoisted to the program as a synthetic `ClassDecl` whose name marks it as anonymous.
#[test]
fn test_parse_anonymous_class_hoists_declaration() {
    let stmts = parse_source(
        "<?php $o = new class { public function v(): string { return \"x\"; } };",
    );
    // The assignment plus the hoisted synthetic class declaration appended to the program.
    assert_eq!(stmts.len(), 2);
    let synthetic_name = match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::NewObject { class_name, args } => {
                assert!(args.is_empty());
                assert!(
                    class_name.as_str().starts_with("class@anonymous"),
                    "expected anonymous synthetic name, got {}",
                    class_name.as_str()
                );
                class_name.as_str().to_string()
            }
            other => panic!("Expected NewObject, got {:?}", other),
        },
        other => panic!("Expected Assign, got {:?}", other),
    };
    match &stmts[1].kind {
        StmtKind::ClassDecl { name, methods, .. } => {
            assert_eq!(name, &synthetic_name);
            assert_eq!(methods[0].name, "v");
        }
        other => panic!("Expected hoisted ClassDecl, got {:?}", other),
    }
}

/// Verifies that `new class(args) extends P implements I {}` carries constructor args, the parent,
/// and the interface list onto the hoisted declaration.
#[test]
fn test_parse_anonymous_class_with_ctor_extends_implements() {
    let stmts = parse_source(
        "<?php interface I {} class P {} $o = new class(1, 2) extends P implements I { public function __construct(int $a, int $b) {} };",
    );
    // interface, parent class, the assignment, then the hoisted anonymous class.
    let assign = stmts
        .iter()
        .find(|s| matches!(s.kind, StmtKind::Assign { .. }))
        .expect("assignment present");
    match &assign.kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::NewObject { args, .. } => assert_eq!(args.len(), 2),
            other => panic!("Expected NewObject, got {:?}", other),
        },
        _ => unreachable!(),
    }
    let anon = stmts
        .iter()
        .filter_map(|s| match &s.kind {
            StmtKind::ClassDecl {
                name,
                extends,
                implements,
                ..
            } if name.starts_with("class@anonymous") => Some((extends, implements)),
            _ => None,
        })
        .next()
        .expect("hoisted anonymous class present");
    assert_eq!(anon.0.as_deref(), Some("P"));
    assert_eq!(anon.1.len(), 1);
}
