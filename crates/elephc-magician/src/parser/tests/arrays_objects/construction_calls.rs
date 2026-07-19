//! Purpose:
//! Parser tests for named/dynamic/anonymous object construction, clone, and
//! method call argument forms.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Parenthesis-free construction and named call arguments are retained.

use super::super::support::*;

/// Verifies object construction parses as a named EvalIR expression.
#[test]
fn parse_fragment_accepts_new_object_source() {
    let program = parse_fragment(br#"return new Box();"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NewObject {
            class_name: "Box".to_string(),
            args: Vec::new(),
        }))]
    );
}
/// Verifies object construction accepts a runtime class-name variable.
#[test]
fn parse_fragment_accepts_dynamic_new_object_source() {
    let program =
        parse_fragment(br#"return new $className("Ada");"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::DynamicNewObject {
            class_name: Box::new(EvalExpr::LoadVar("className".to_string())),
            args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                "Ada".to_string()
            )))],
        }))]
    );
}
/// Verifies object construction accepts a parenthesized runtime class-name expression.
#[test]
fn parse_fragment_accepts_expression_new_object_source() {
    let program =
        parse_fragment(br#"return new ($factory->className)("Ada");"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::DynamicNewObject {
            class_name: Box::new(EvalExpr::PropertyGet {
                object: Box::new(EvalExpr::LoadVar("factory".to_string())),
                property: "className".to_string(),
            }),
            args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
                "Ada".to_string()
            )))],
        }))]
    );
}
/// Verifies PHP constructor parentheses are optional for named and runtime class targets.
#[test]
fn parse_fragment_accepts_new_object_without_constructor_parentheses_source() {
    let named = parse_fragment(br#"return new Box;"#).expect("fragment should parse");
    assert_eq!(
        named.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NewObject {
            class_name: "Box".to_string(),
            args: Vec::new(),
        }))]
    );

    let dynamic = parse_fragment(br#"return new $className;"#).expect("fragment should parse");
    assert_eq!(
        dynamic.statements(),
        &[EvalStmt::Return(Some(EvalExpr::DynamicNewObject {
            class_name: Box::new(EvalExpr::LoadVar("className".to_string())),
            args: Vec::new(),
        }))]
    );
}
/// Verifies object construction accepts explicitly qualified class names.
#[test]
fn parse_fragment_accepts_qualified_new_object_source() {
    let program = parse_fragment(br#"return new \EvalNs\Box();"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NewObject {
            class_name: "EvalNs\\Box".to_string(),
            args: Vec::new(),
        }))]
    );
}

/// Verifies clone expressions parse as unary object expressions.
#[test]
fn parse_fragment_accepts_clone_expression_source() {
    let program = parse_fragment(br#"return clone $box;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Clone(Box::new(
            EvalExpr::LoadVar("box".to_string())
        ))))]
    );
}

/// Verifies anonymous class expressions parse as executable eval class metadata.
#[test]
fn parse_fragment_accepts_anonymous_class_source() {
    let program = parse_fragment(
        br#"return new readonly class("Ada") extends BaseBox implements Labelled {
    public string $name;
    public function label() { return $this->name; }
};"#,
    )
    .expect("fragment should parse");
    let [EvalStmt::Return(Some(EvalExpr::NewAnonymousClass { class, args }))] =
        program.statements()
    else {
        panic!("expected anonymous class return");
    };

    assert!(class.name().starts_with("class@anonymous#eval"));
    assert!(class.is_anonymous());
    assert!(class.is_readonly_class());
    assert_eq!(class.parent(), Some("BaseBox"));
    assert_eq!(class.interfaces(), &["Labelled".to_string()]);
    assert_eq!(class.properties().len(), 1);
    assert_eq!(class.methods().len(), 1);
    assert_eq!(
        args,
        &[EvalCallArg::positional(EvalExpr::Const(EvalConst::String(
            "Ada".to_string(),
        )))]
    );
}

/// Verifies object method calls preserve source-order argument expressions.
#[test]
fn parse_fragment_accepts_method_call_args_source() {
    let program = parse_fragment(br#"return $this->add($x + 1);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::MethodCall {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            method: "add".to_string(),
            args: vec![EvalCallArg::positional(EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::LoadVar("x".to_string())),
                right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
            })],
        }))]
    );
}
/// Verifies object method calls parse multiple argument expressions in source order.
#[test]
fn parse_fragment_accepts_method_call_multiple_args_source() {
    let program =
        parse_fragment(br#"return $this->label($x, "ok");"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::MethodCall {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            method: "label".to_string(),
            args: vec![
                EvalCallArg::positional(EvalExpr::LoadVar("x".to_string())),
                EvalCallArg::positional(EvalExpr::Const(EvalConst::String("ok".to_string()))),
            ],
        }))]
    );
}

/// Verifies object method calls preserve named arguments in source order.
#[test]
fn parse_fragment_accepts_named_method_call_args_source() {
    let program = parse_fragment(br#"return $this->label(right: "ok", left: $x);"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::MethodCall {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            method: "label".to_string(),
            args: vec![
                EvalCallArg::named(
                    "right",
                    EvalExpr::Const(EvalConst::String("ok".to_string())),
                ),
                EvalCallArg::named("left", EvalExpr::LoadVar("x".to_string())),
            ],
        }))]
    );
}
