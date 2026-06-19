//! Purpose:
//! Parser tests for arrays, array writes, object properties, methods, and object construction.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases cover postfix and aggregate expression syntax.

use super::support::*;

/// Verifies indexed array literals and reads parse as runtime array expressions.
#[test]
fn parse_fragment_accepts_indexed_array_read_source() {
    let program = parse_fragment(br#"return [1, 2][0];"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::ArrayGet {
            array: Box::new(EvalExpr::Array(vec![
                EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(1))),
                EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(2))),
            ])),
            index: Box::new(EvalExpr::Const(EvalConst::Int(0))),
        }))]
    );
}
/// Verifies legacy `array(...)` literals parse through the same EvalIR array node.
#[test]
fn parse_fragment_accepts_legacy_array_literal_source() {
    let program =
        parse_fragment(br#"return array(1, "name" => "Ada",)[1];"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::ArrayGet {
            array: Box::new(EvalExpr::Array(vec![
                EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(1))),
                EvalArrayElement::KeyValue {
                    key: EvalExpr::Const(EvalConst::String("name".to_string())),
                    value: EvalExpr::Const(EvalConst::String("Ada".to_string())),
                },
            ])),
            index: Box::new(EvalExpr::Const(EvalConst::Int(1))),
        }))]
    );
}
/// Verifies associative array literals preserve explicit key/value expressions.
#[test]
fn parse_fragment_accepts_assoc_array_literal_source() {
    let program = parse_fragment(br#"return ["name" => "Ada"];"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Array(vec![
            EvalArrayElement::KeyValue {
                key: EvalExpr::Const(EvalConst::String("name".to_string())),
                value: EvalExpr::Const(EvalConst::String("Ada".to_string())),
            }
        ])))]
    );
}
/// Verifies indexed array writes parse as variable-target array set statements.
#[test]
fn parse_fragment_accepts_indexed_array_write_source() {
    let program = parse_fragment(br#"$items[1] = "x";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ArraySetVar {
            name: "items".to_string(),
            index: EvalExpr::Const(EvalConst::Int(1)),
            value: EvalExpr::Const(EvalConst::String("x".to_string())),
        }]
    );
}
/// Verifies indexed array append syntax parses as a variable-target append statement.
#[test]
fn parse_fragment_accepts_indexed_array_append_source() {
    let program = parse_fragment(br#"$items[] = "x";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ArrayAppendVar {
            name: "items".to_string(),
            value: EvalExpr::Const(EvalConst::String("x".to_string())),
        }]
    );
}
/// Verifies array append syntax is accepted inside `for` update clauses.
#[test]
fn parse_fragment_accepts_array_append_in_for_update_source() {
    let program = parse_fragment(br#"for ($i = 0; $i < 2; $items[] = $i) { $i += 1; }"#)
        .expect("fragment should parse");
    let [EvalStmt::For { update, .. }] = program.statements() else {
        panic!("expected for statement");
    };
    assert_eq!(
        update,
        &vec![EvalStmt::ArrayAppendVar {
            name: "items".to_string(),
            value: EvalExpr::LoadVar("i".to_string()),
        }]
    );
}
/// Verifies object property reads parse as postfix EvalIR expressions.
#[test]
fn parse_fragment_accepts_property_read_source() {
    let program = parse_fragment(br#"return $this->x;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::PropertyGet {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            property: "x".to_string(),
        }))]
    );
}
/// Verifies property names preserve source case while keywords remain case-insensitive.
#[test]
fn parse_fragment_preserves_property_case_source() {
    let program = parse_fragment(br#"RETURN $this->camelName;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::PropertyGet {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            property: "camelName".to_string(),
        }))]
    );
}
/// Verifies object method calls parse as postfix EvalIR call expressions.
#[test]
fn parse_fragment_accepts_method_call_source() {
    let program = parse_fragment(br#"return $this->Answer();"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::MethodCall {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            method: "answer".to_string(),
            args: Vec::new(),
        }))]
    );
}
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

/// Verifies object property writes parse as dedicated EvalIR statements.
#[test]
fn parse_fragment_accepts_property_write_source() {
    let program = parse_fragment(br#"$this->x = $this->x + 1;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::PropertySet {
            object: EvalExpr::LoadVar("this".to_string()),
            property: "x".to_string(),
            value: EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::PropertyGet {
                    object: Box::new(EvalExpr::LoadVar("this".to_string())),
                    property: "x".to_string(),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
            },
        }]
    );
}
