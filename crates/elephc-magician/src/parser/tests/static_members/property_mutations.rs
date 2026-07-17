//! Purpose:
//! Parser tests for static-property assignment, compound operations, unset,
//! increment/decrement, reference binding, and array writes.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Named and dynamic class/property receivers retain dedicated EvalIR forms.

use super::super::support::*;

/// Verifies runtime-valued static receivers support property writes.
#[test]
fn parse_fragment_accepts_dynamic_static_property_assignment() {
    let program = parse_fragment(br#"$class::$count = 2;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::DynamicStaticPropertySet {
            class_name: EvalExpr::LoadVar("class".to_string()),
            property: "count".to_string(),
            value: EvalExpr::Const(EvalConst::Int(2)),
        }]
    );
}

/// Verifies dynamic static property compound assignments lower to read-modify-write EvalIR.
#[test]
fn parse_fragment_accepts_dynamic_static_property_compound_assignment() {
    let program = parse_fragment(br#"$class::$count += 2;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::DynamicStaticPropertySet {
            class_name: EvalExpr::LoadVar("class".to_string()),
            property: "count".to_string(),
            value: EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::DynamicStaticPropertyGet {
                    class_name: Box::new(EvalExpr::LoadVar("class".to_string())),
                    property: "count".to_string(),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
            },
        }]
    );
}

/// Verifies static property unsets parse as explicit static-unset statements.
#[test]
fn parse_fragment_accepts_static_property_unset() {
    let program =
        parse_fragment(br#"unset(EvalStaticBox::$count);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::UnsetStaticProperty {
            class_name: "EvalStaticBox".to_string(),
            property: "count".to_string(),
        }]
    );
}

/// Verifies runtime-valued static property unsets preserve the class receiver expression.
#[test]
fn parse_fragment_accepts_dynamic_static_property_unset() {
    let program = parse_fragment(br#"unset($class::$count, $class::${$name});"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::UnsetDynamicStaticProperty {
                class_name: EvalExpr::LoadVar("class".to_string()),
                property: "count".to_string(),
            },
            EvalStmt::UnsetDynamicStaticPropertyName {
                class_name: EvalExpr::LoadVar("class".to_string()),
                property: EvalExpr::LoadVar("name".to_string()),
            },
        ]
    );
}

/// Verifies static-property array element unset preserves the static member expression.
#[test]
fn parse_fragment_accepts_static_property_array_unset() {
    let program = parse_fragment(br#"unset(EvalStaticBox::$items[0], $class::$items[1]);"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::UnsetArrayElement {
                array: EvalExpr::StaticPropertyGet {
                    class_name: "EvalStaticBox".to_string(),
                    property: "items".to_string(),
                },
                index: EvalExpr::Const(EvalConst::Int(0)),
            },
            EvalStmt::UnsetArrayElement {
                array: EvalExpr::DynamicStaticPropertyGet {
                    class_name: Box::new(EvalExpr::LoadVar("class".to_string())),
                    property: "items".to_string(),
                },
                index: EvalExpr::Const(EvalConst::Int(1)),
            },
        ]
    );
}

/// Verifies static property increment/decrement parse as dedicated member updates.
#[test]
fn parse_fragment_accepts_static_property_inc_dec() {
    let program = parse_fragment(br#"EvalStaticBox::$count++; --EvalStaticBox::$count;"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::StaticPropertyIncDec {
                class_name: "EvalStaticBox".to_string(),
                property: "count".to_string(),
                increment: true,
            },
            EvalStmt::StaticPropertyIncDec {
                class_name: "EvalStaticBox".to_string(),
                property: "count".to_string(),
                increment: false,
            },
        ]
    );
}

/// Verifies dynamic static property increment/decrement preserves the receiver expression.
#[test]
fn parse_fragment_accepts_dynamic_static_property_inc_dec() {
    let program =
        parse_fragment(br#"$class::$count++; --$class::$count;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::DynamicStaticPropertyIncDec {
                class_name: EvalExpr::LoadVar("class".to_string()),
                property: "count".to_string(),
                increment: true,
            },
            EvalStmt::DynamicStaticPropertyIncDec {
                class_name: EvalExpr::LoadVar("class".to_string()),
                property: "count".to_string(),
                increment: false,
            },
        ]
    );
}

/// Verifies static property compound assignments lower to one read-modify-write statement.
#[test]
fn parse_fragment_accepts_static_property_compound_assignment() {
    let program = parse_fragment(br#"EvalStaticBox::$count += 2;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::StaticPropertySet {
            class_name: "EvalStaticBox".to_string(),
            property: "count".to_string(),
            value: EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::StaticPropertyGet {
                    class_name: "EvalStaticBox".to_string(),
                    property: "count".to_string(),
                }),
                right: Box::new(EvalExpr::Const(EvalConst::Int(2))),
            },
        }]
    );
}

/// Verifies static property reference bindings parse for named and dynamic targets.
#[test]
fn parse_fragment_accepts_static_property_reference_bind_source() {
    let program = parse_fragment(
        br#"EvalStaticBox::$count =& $source;
$class::$count =& $dynamic;
EvalStaticBox::${$name} =& $other;
(factory())::${$name} =& $third;"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::StaticPropertyReferenceBind {
                class_name: "EvalStaticBox".to_string(),
                property: "count".to_string(),
                source: "source".to_string(),
            },
            EvalStmt::DynamicStaticPropertyReferenceBind {
                class_name: EvalExpr::LoadVar("class".to_string()),
                property: "count".to_string(),
                source: "dynamic".to_string(),
            },
            EvalStmt::DynamicStaticPropertyNameReferenceBind {
                class_name: EvalExpr::ClassNameFetch {
                    class_name: "EvalStaticBox".to_string(),
                },
                property: EvalExpr::LoadVar("name".to_string()),
                source: "other".to_string(),
            },
            EvalStmt::DynamicStaticPropertyNameReferenceBind {
                class_name: EvalExpr::Call {
                    name: "factory".to_string(),
                    args: Vec::new(),
                },
                property: EvalExpr::LoadVar("name".to_string()),
                source: "third".to_string(),
            },
        ]
    );
}

/// Verifies indexed static-property writes parse as dedicated EvalIR statements.
#[test]
fn parse_fragment_accepts_static_property_array_write_source() {
    let program =
        parse_fragment(br#"EvalStaticBox::$items[0] = "x"; EvalStaticBox::$items[] = "y";"#)
            .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::StaticPropertyArraySet {
                class_name: "EvalStaticBox".to_string(),
                property: "items".to_string(),
                index: EvalExpr::Const(EvalConst::Int(0)),
                op: None,
                value: EvalExpr::Const(EvalConst::String("x".to_string())),
            },
            EvalStmt::StaticPropertyArrayAppend {
                class_name: "EvalStaticBox".to_string(),
                property: "items".to_string(),
                value: EvalExpr::Const(EvalConst::String("y".to_string())),
            },
        ]
    );
}

/// Verifies indexed static-property compound assignment retains the static target.
#[test]
fn parse_fragment_accepts_static_property_array_compound_assignment_source() {
    let program =
        parse_fragment(br#"EvalStaticBox::$items[0] .= "x";"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::StaticPropertyArraySet {
            class_name: "EvalStaticBox".to_string(),
            property: "items".to_string(),
            index: EvalExpr::Const(EvalConst::Int(0)),
            op: Some(EvalBinOp::Concat),
            value: EvalExpr::Const(EvalConst::String("x".to_string())),
        }]
    );
}

/// Verifies runtime-valued static receivers support indexed static-property writes.
#[test]
fn parse_fragment_accepts_dynamic_static_property_array_write_source() {
    let program = parse_fragment(br#"$class::$items[0] = "x"; $class::$items[] = "y";"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::DynamicStaticPropertyArraySet {
                class_name: EvalExpr::LoadVar("class".to_string()),
                property: "items".to_string(),
                index: EvalExpr::Const(EvalConst::Int(0)),
                op: None,
                value: EvalExpr::Const(EvalConst::String("x".to_string())),
            },
            EvalStmt::DynamicStaticPropertyArrayAppend {
                class_name: EvalExpr::LoadVar("class".to_string()),
                property: "items".to_string(),
                value: EvalExpr::Const(EvalConst::String("y".to_string())),
            },
        ]
    );
}
