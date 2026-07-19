//! Purpose:
//! Parser tests for dynamic static-property names and runtime-valued metadata
//! receivers.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Braced runtime names lower to explicit EvalIR name expressions.

use super::super::support::*;

/// Verifies `${...}` static property names parse as runtime-name expressions.
#[test]
fn parse_fragment_accepts_dynamic_static_property_name_expressions() {
    let program = parse_fragment(
        br#"return [EvalStaticBox::${$property}, $class::${name_expr()}, (factory())::${"items"}];"#,
    )
    .expect("fragment should parse");
    let name_expr_call = || EvalExpr::Call {
        name: "name_expr".to_string(),
        args: Vec::new(),
    };
    let factory_call = || EvalExpr::Call {
        name: "factory".to_string(),
        args: Vec::new(),
    };
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Array(vec![
            EvalArrayElement::Value(EvalExpr::DynamicStaticPropertyNameGet {
                class_name: Box::new(EvalExpr::ClassNameFetch {
                    class_name: "EvalStaticBox".to_string(),
                }),
                property: Box::new(EvalExpr::LoadVar("property".to_string())),
            }),
            EvalArrayElement::Value(EvalExpr::DynamicStaticPropertyNameGet {
                class_name: Box::new(EvalExpr::LoadVar("class".to_string())),
                property: Box::new(name_expr_call()),
            }),
            EvalArrayElement::Value(EvalExpr::DynamicStaticPropertyNameGet {
                class_name: Box::new(factory_call()),
                property: Box::new(EvalExpr::Const(EvalConst::String("items".to_string()))),
            }),
        ])))]
    );
}

/// Verifies `${...}` static property names lower write-like statements.
#[test]
fn parse_fragment_accepts_dynamic_static_property_name_writes() {
    let program = parse_fragment(
        br#"EvalStaticBox::${$property} = 2;
++$class::${name_expr()};
(factory())::${"items"}[] = 4;"#,
    )
    .expect("fragment should parse");
    let name_expr_call = || EvalExpr::Call {
        name: "name_expr".to_string(),
        args: Vec::new(),
    };
    let factory_call = || EvalExpr::Call {
        name: "factory".to_string(),
        args: Vec::new(),
    };
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::DynamicStaticPropertyNameSet {
                class_name: EvalExpr::ClassNameFetch {
                    class_name: "EvalStaticBox".to_string(),
                },
                property: EvalExpr::LoadVar("property".to_string()),
                value: EvalExpr::Const(EvalConst::Int(2)),
            },
            EvalStmt::DynamicStaticPropertyNameIncDec {
                class_name: EvalExpr::LoadVar("class".to_string()),
                property: name_expr_call(),
                increment: true,
            },
            EvalStmt::DynamicStaticPropertyNameArrayAppend {
                class_name: factory_call(),
                property: EvalExpr::Const(EvalConst::String("items".to_string())),
                value: EvalExpr::Const(EvalConst::Int(4)),
            },
        ]
    );
}

/// Verifies runtime-valued static receivers support properties, constants, and `::class`.
#[test]
fn parse_fragment_accepts_dynamic_static_metadata_receiver() {
    let program = parse_fragment(br#"return $class::$count . $class::VALUE . $object::class;"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(EvalExpr::DynamicStaticPropertyGet {
                    class_name: Box::new(EvalExpr::LoadVar("class".to_string())),
                    property: "count".to_string(),
                }),
                right: Box::new(EvalExpr::DynamicClassConstantFetch {
                    class_name: Box::new(EvalExpr::LoadVar("class".to_string())),
                    constant: "VALUE".to_string(),
                }),
            }),
            right: Box::new(EvalExpr::DynamicClassNameFetch {
                class_name: Box::new(EvalExpr::LoadVar("object".to_string())),
            }),
        }))]
    );
}
