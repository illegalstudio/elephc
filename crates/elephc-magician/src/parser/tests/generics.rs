//! Purpose:
//! Parser coverage for retained generic declarations, nested type lists, and comparisons.
//!
//! Called from:
//! - `cargo test -p elephc-magician generics`.
//!
//! Key details:
//! - Right shifts split only while parsing type arguments.

use super::support::*;

/// An incomplete type-list probe leaves PHP comparisons and native-syntax provenance intact.
#[test]
fn generics_review_instanceof_comparisons() {
    for right in ["1", "$limit", "LIMIT", "limit()", "(1)"] {
        let source = format!("echo $value instanceof C < {right};");
        let program = parse_fragment(source.as_bytes()).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(matches!(&program.statements()[0], EvalStmt::Echo(EvalExpr::Binary { op: EvalBinOp::Lt, left, .. })
            if matches!(left.as_ref(), EvalExpr::InstanceOf { .. })), "{source}");
        assert!(!program.native_generic_syntax(), "{source}");
    }
    let program = parse_fragment(b"echo new C < $limit;").unwrap();
    assert!(!program.native_generic_syntax());
}

/// Complete nested class type lists retain specialization before subsequent comparisons and shifts.
#[test]
fn generics_review_instanceof_complete_type_lists() {
    for source in [
        "echo $value instanceof Box<int> < 2;",
        "echo $value instanceof Box<array<array<int>>> >> 1;",
        "echo new Box<int>(1) instanceof Box<int>;",
    ] {
        let program = parse_fragment(source.as_bytes()).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(program.native_generic_syntax(), "{source}");
    }
}

/// Every class-like keyword spelling retains scoped PHPDoc member types in its prototype.
#[test]
fn generics_docblock_classlike_keywords_are_case_insensitive() {
    for keyword in ["CLASS", "cLaSs", "TRAIT", "TrAiT", "INTERFACE", "InTeRfAcE"] {
        let body = if keyword.eq_ignore_ascii_case("interface") {
            "/** @return T */ public function get();"
        } else { "/** @var T */ public $value; /** @return T */ public function get() { return $this->value; }" };
        let program = parse_fragment(format!("{keyword} Box<T> {{ {body} }}").as_bytes()).unwrap();
        let EvalStmt::GenericDecl(template) = &program.statements()[0] else { panic!("expected generic prototype"); };
        let (ret, property) = match template.prototype.as_ref() {
            EvalStmt::ClassDecl(class) => (class.methods()[0].return_type(), class.properties()[0].property_type()),
            EvalStmt::TraitDecl(trait_decl) => (trait_decl.methods()[0].return_type(), trait_decl.properties()[0].property_type()),
            EvalStmt::InterfaceDecl(interface) => (interface.methods()[0].return_type(), None),
            _ => panic!("expected class-like prototype"),
        };
        assert_eq!(crate::eval_ir::generic_type(ret.expect("annotated return")).to_string(), "T", "{keyword}");
        if !keyword.eq_ignore_ascii_case("interface") { assert_eq!(crate::eval_ir::generic_type(property.expect("annotated property")).to_string(), "T", "{keyword}"); }
    }
}

/// Imported generic calls and first-class callables retain resolved spelling and written slots.
#[test]
fn generics_function_names_preserve_imports_and_source_spelling() {
    let program = parse_fragment(b"namespace App; use function Lib\\Identity as Alias; aLIAS<string>('ok'); ALias<int>(...); Missing<string>(...);").unwrap();
    assert!(matches!(&program.statements()[0], EvalStmt::Expr(EvalExpr::Call { name, .. })
        if name == "Lib\\Identity<string>"));
    assert_eq!(program.statements()[1], EvalStmt::Expr(EvalExpr::FunctionCallable {
        name: "Lib\\Identity<int>".to_string(), fallback_name: None,
    }));
    assert_eq!(program.statements()[2], EvalStmt::Expr(EvalExpr::FunctionCallable {
        name: "App\\Missing<string>".to_string(), fallback_name: Some("Missing<string>".to_string()),
    }));
}

/// Typed-local probing must not reinterpret reserved statement keywords as class types.
#[test]
fn generics_typed_local_keeps_statement_keyword_precedence() {
    for source in ["echo $value = 1;", "return $value = 1;"] {
        assert!(parse_fragment(source.as_bytes()).is_err(), "{source}");
    }
}

/// Native function/class/interface/trait declarations retain templates.
#[test]
fn generics_parse_declaration_families() {
    for source in [
        "function id<T>(T $v): T { return $v; }",
        "class Box<T> { public T $value; }",
        "interface Read<T> { public function get(): T; }",
        "trait Store<T> { public T $value; }",
    ] {
        let program = parse_fragment(source.as_bytes()).unwrap();
        assert!(matches!(&program.statements()[0], EvalStmt::GenericDecl(_)), "{source}");
    }
}

/// Nested generic closes coexist with ordinary PHP comparison and shift operators.
#[test]
fn generics_nested_lists_leave_comparisons_intact() {
    for source in ["new Box<Box<int>>(new Box<int>(1));", "echo id<array<array<int>>>([[1]])[0][0];", "echo $a < $b && $b > $c;", "echo 16 >> 2;"] {
        parse_fragment(source.as_bytes()).unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Malformed generic lists and invalid associative array keys fail during parsing.
#[test]
fn generics_malformed_lists_are_rejected() {
    for source in ["enum Choice<T> { case None; }", "function id<>(int $v) {}", "class Box<T, T> {}", "function id<T(T $v) {}", "function f(array<float, int> $v) {}"] {
        assert!(parse_fragment(source.as_bytes()).is_err(), "{source}");
    }
}

/// PHPDoc arguments attach only to the qualified parent named by the matching clause.
#[test]
fn generics_docblock_parent_arguments_match_qualified_clauses() {
    for (annotations, parent, interface) in [
        ("@extends \\Ns\\Foo<int>", "Ns\\Foo<int>", "Other\\Foo"),
        ("@implements \\Other\\Foo<string>", "Ns\\Foo", "Other\\Foo<string>"),
        ("@extends \\Ns\\Foo<int>\n * @implements \\Other\\Foo<string>", "Ns\\Foo<int>", "Other\\Foo<string>"),
        ("@extends \\Other\\Foo<int>\n * @implements \\Ns\\Foo<string>", "Ns\\Foo", "Other\\Foo"),
    ] {
        let source = format!("/**\n * {annotations}\n */ class Child extends \\Ns\\Foo implements \\Other\\Foo {{}}");
        let program = parse_fragment(source.as_bytes()).unwrap();
        let EvalStmt::ClassDecl(class) = &program.statements()[0] else { panic!("expected class"); };
        assert_eq!(class.parent(), Some(parent), "{source}");
        assert_eq!(class.interfaces(), &[interface.to_string()], "{source}");
    }
}

/// Independent interface parents with the same leaf name receive one argument list each.
#[test]
fn generics_docblock_interface_parents_keep_separate_arguments() {
    let source = b"/**\n * @extends \\Ns\\Foo<int>\n * @extends \\Other\\Foo<string>\n */ interface Child extends \\Ns\\Foo, \\Other\\Foo {}";
    let program = parse_fragment(source).unwrap();
    let EvalStmt::InterfaceDecl(interface) = &program.statements()[0] else { panic!("expected interface"); };
    assert_eq!(interface.parents(), &["Ns\\Foo<int>".to_string(), "Other\\Foo<string>".to_string()]);
}

/// Native parent arguments win and duplicate PHPDoc tags do not create suffix chains.
#[test]
fn generics_docblock_parent_arguments_keep_native_precedence() {
    for (header, expected) in [
        ("\\Ns\\Foo<string>", "Ns\\Foo<string>"),
        ("\\Ns\\Foo", "Ns\\Foo<int>"),
    ] {
        let source = format!("/**\n * @extends \\Ns\\Foo<int>\n * @extends \\Ns\\Foo<int>\n */ class Child extends {header} {{}}");
        let program = parse_fragment(source.as_bytes()).unwrap();
        let EvalStmt::ClassDecl(class) = &program.statements()[0] else { panic!("expected class"); };
        assert_eq!(class.parent(), Some(expected));
    }
}
