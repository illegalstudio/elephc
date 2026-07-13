//! Purpose:
//! Parser tests for array literals/access and object property, method, dynamic,
//! and nullsafe access expressions.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover postfix and aggregate expression syntax.

use super::super::support::*;

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

/// Verifies array literals preserve by-reference element syntax.
#[test]
fn parse_fragment_accepts_array_reference_elements_source() {
    let program = parse_fragment(br#"return [&$value, "named" => &$other];"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Array(vec![
            EvalArrayElement::Reference(EvalExpr::LoadVar("value".to_string())),
            EvalArrayElement::KeyReference {
                key: EvalExpr::Const(EvalConst::String("named".to_string())),
                value: EvalExpr::LoadVar("other".to_string()),
            },
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
            method: "Answer".to_string(),
            args: Vec::new(),
        }))]
    );
}
/// Verifies first-class object method syntax keeps callable-capture semantics in EvalIR.
#[test]
fn parse_fragment_accepts_first_class_method_callable_source() {
    let program = parse_fragment(br#"return $this->Answer(...);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::MethodCallable {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            method: Box::new(EvalExpr::Const(EvalConst::String("Answer".to_string()))),
        }))]
    );
}
/// Verifies braced dynamic object property reads parse as runtime-name EvalIR expressions.
#[test]
fn parse_fragment_accepts_dynamic_property_read_source() {
    let program = parse_fragment(br#"return $this->{$name};"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::DynamicPropertyGet {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            property: Box::new(EvalExpr::LoadVar("name".to_string())),
        }))]
    );
}
/// Verifies variable-name dynamic object method calls parse as runtime-name EvalIR calls.
#[test]
fn parse_fragment_accepts_dynamic_method_call_source() {
    let program = parse_fragment(br#"return $this->$method();"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::DynamicMethodCall {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            method: Box::new(EvalExpr::LoadVar("method".to_string())),
            args: Vec::new(),
        }))]
    );
}
/// Verifies nullsafe object property reads parse as dedicated postfix EvalIR expressions.
#[test]
fn parse_fragment_accepts_nullsafe_property_read_source() {
    let program = parse_fragment(br#"return $this?->x;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NullsafePropertyGet {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            property: "x".to_string(),
        }))]
    );
}
/// Verifies nullsafe object method calls parse as dedicated postfix EvalIR call expressions.
#[test]
fn parse_fragment_accepts_nullsafe_method_call_source() {
    let program = parse_fragment(br#"return $this?->Answer();"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NullsafeMethodCall {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            method: "Answer".to_string(),
            args: Vec::new(),
        }))]
    );
}
/// Verifies nullsafe braced dynamic property reads parse as runtime-name EvalIR expressions.
#[test]
fn parse_fragment_accepts_nullsafe_dynamic_property_read_source() {
    let program = parse_fragment(br#"return $this?->{$name};"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NullsafeDynamicPropertyGet {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            property: Box::new(EvalExpr::LoadVar("name".to_string())),
        }))]
    );
}
/// Verifies nullsafe dynamic method calls parse as runtime-name EvalIR call expressions.
#[test]
fn parse_fragment_accepts_nullsafe_dynamic_method_call_source() {
    let program = parse_fragment(br#"return $this?->$method();"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::NullsafeDynamicMethodCall {
            object: Box::new(EvalExpr::LoadVar("this".to_string())),
            method: Box::new(EvalExpr::LoadVar("method".to_string())),
            args: Vec::new(),
        }))]
    );
}
