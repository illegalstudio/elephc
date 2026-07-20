//! Purpose:
//! Parser tests for static declarations, calls, first-class callables, dynamic
//! receivers, and expression receivers.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases verify `::` receivers lower to EvalIR static-member nodes.

use super::super::support::*;

/// Verifies static class members lower with explicit static metadata.
#[test]
fn parse_fragment_accepts_static_class_members() {
    let program = parse_fragment(
        br#"class EvalStaticBox {
    public static int $count = 1;
    public static function read() { return self::$count; }
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::new(
            "EvalStaticBox",
            vec![EvalClassProperty::with_visibility_and_static(
                "count",
                EvalVisibility::Public,
                true,
                Some(EvalExpr::Const(EvalConst::Int(1))),
            )
            .with_type(Some(EvalParameterType::new(
                vec![EvalParameterTypeVariant::Int],
                false,
            )))],
            vec![EvalClassMethod::with_visibility_and_modifiers(
                "read",
                EvalVisibility::Public,
                true,
                false,
                false,
                Vec::new(),
                vec![EvalStmt::Return(Some(EvalExpr::StaticPropertyGet {
                    class_name: "self".to_string(),
                    property: "count".to_string(),
                }))],
            )],
        ))]
    );
}

/// Verifies static method calls lower to EvalIR call expressions.
#[test]
fn parse_fragment_accepts_static_method_call_expression() {
    let program =
        parse_fragment(br#"return EvalStaticBox::Read(2);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::StaticMethodCall {
            class_name: "EvalStaticBox".to_string(),
            method: "Read".to_string(),
            args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(2)))],
        }))]
    );
}

/// Verifies first-class static method syntax retains static callable metadata.
#[test]
fn parse_fragment_accepts_first_class_static_method_callable_source() {
    let program =
        parse_fragment(br#"return EvalStaticBox::Read(...);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::StaticMethodCallable {
            class_name: "EvalStaticBox".to_string(),
            method: Box::new(EvalExpr::Const(EvalConst::String("Read".to_string()))),
        }))]
    );
}

/// Verifies static method calls preserve named arguments in source order.
#[test]
fn parse_fragment_accepts_named_static_method_call_expression() {
    let program =
        parse_fragment(br#"return EvalStaticBox::Read(step: 2);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::StaticMethodCall {
            class_name: "EvalStaticBox".to_string(),
            method: "Read".to_string(),
            args: vec![EvalCallArg::named(
                "step",
                EvalExpr::Const(EvalConst::Int(2)),
            )],
        }))]
    );
}

/// Verifies runtime-valued static receivers lower to dynamic static method calls.
#[test]
fn parse_fragment_accepts_dynamic_static_method_receiver() {
    let program =
        parse_fragment(br#"return $class::Read(step: 2);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::DynamicStaticMethodCall {
            class_name: Box::new(EvalExpr::LoadVar("class".to_string())),
            method: Box::new(EvalExpr::Const(EvalConst::String("Read".to_string()))),
            args: vec![EvalCallArg::named(
                "step",
                EvalExpr::Const(EvalConst::Int(2)),
            )],
        }))]
    );
}

/// Verifies runtime-valued static receivers support variable method names.
#[test]
fn parse_fragment_accepts_dynamic_static_method_name() {
    let program =
        parse_fragment(br#"return $class::$method(2);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::DynamicStaticMethodCall {
            class_name: Box::new(EvalExpr::LoadVar("class".to_string())),
            method: Box::new(EvalExpr::LoadVar("method".to_string())),
            args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(2)))],
        }))]
    );
}

/// Verifies named static receivers support variable method names.
#[test]
fn parse_fragment_accepts_named_receiver_dynamic_static_method_name() {
    let program =
        parse_fragment(br#"return EvalStaticBox::$method(2);"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::DynamicStaticMethodCall {
            class_name: Box::new(EvalExpr::ClassNameFetch {
                class_name: "EvalStaticBox".to_string(),
            }),
            method: Box::new(EvalExpr::LoadVar("method".to_string())),
            args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(2)))],
        }))]
    );
}

/// Verifies braced dynamic static method names preserve their method expression.
#[test]
fn parse_fragment_accepts_braced_dynamic_static_method_name() {
    let program =
        parse_fragment(br#"return EvalStaticBox::{$method}(2) . $class::{"Read"}(3);"#)
            .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::DynamicStaticMethodCall {
                class_name: Box::new(EvalExpr::ClassNameFetch {
                    class_name: "EvalStaticBox".to_string(),
                }),
                method: Box::new(EvalExpr::LoadVar("method".to_string())),
                args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(2)))],
            }),
            right: Box::new(EvalExpr::DynamicStaticMethodCall {
                class_name: Box::new(EvalExpr::LoadVar("class".to_string())),
                method: Box::new(EvalExpr::Const(EvalConst::String("Read".to_string()))),
                args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(3)))],
            }),
        }))]
    );
}

/// Verifies braced dynamic class constant names preserve their constant-name expression.
#[test]
fn parse_fragment_accepts_braced_dynamic_class_constant_name() {
    let program =
        parse_fragment(br#"return EvalStaticBox::{$constant} . $class::{"VALUE"};"#)
            .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::DynamicClassConstantNameFetch {
                class_name: Box::new(EvalExpr::ClassNameFetch {
                    class_name: "EvalStaticBox".to_string(),
                }),
                constant: Box::new(EvalExpr::LoadVar("constant".to_string())),
            }),
            right: Box::new(EvalExpr::DynamicClassConstantNameFetch {
                class_name: Box::new(EvalExpr::LoadVar("class".to_string())),
                constant: Box::new(EvalExpr::Const(EvalConst::String("VALUE".to_string()))),
            }),
        }))]
    );
}

/// Verifies parenthesized static receivers parse through the dynamic receiver path.
#[test]
fn parse_fragment_accepts_expression_static_receiver_members() {
    let program = parse_fragment(
        br#"return [($class)::read(2), (factory())::VALUE, (factory())::{"VALUE"}, (factory())::$count];"#,
    )
    .expect("fragment should parse");
    let factory_call = || EvalExpr::Call {
        name: "factory".to_string(),
        args: Vec::new(),
    };
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Array(vec![
            EvalArrayElement::Value(EvalExpr::DynamicStaticMethodCall {
                class_name: Box::new(EvalExpr::LoadVar("class".to_string())),
                method: Box::new(EvalExpr::Const(EvalConst::String("read".to_string()))),
                args: vec![EvalCallArg::positional(EvalExpr::Const(EvalConst::Int(2)))],
            }),
            EvalArrayElement::Value(EvalExpr::DynamicClassConstantFetch {
                class_name: Box::new(factory_call()),
                constant: "VALUE".to_string(),
            }),
            EvalArrayElement::Value(EvalExpr::DynamicClassConstantNameFetch {
                class_name: Box::new(factory_call()),
                constant: Box::new(EvalExpr::Const(EvalConst::String("VALUE".to_string()))),
            }),
            EvalArrayElement::Value(EvalExpr::DynamicStaticPropertyGet {
                class_name: Box::new(factory_call()),
                property: "count".to_string(),
            }),
        ])))]
    );
}

/// Verifies expression static receivers support property write statement lowering.
#[test]
fn parse_fragment_accepts_expression_static_receiver_property_writes() {
    let program = parse_fragment(
        br#"(factory())::$count = 2;
(factory())::$count += 3;
(factory())::$items[] = 4;
(factory())::$items[0] = 5;
(factory())::$count++;
++ (factory())::$count;
-- (factory())::$count;"#,
    )
    .expect("fragment should parse");
    let factory_call = || EvalExpr::Call {
        name: "factory".to_string(),
        args: Vec::new(),
    };
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::DynamicStaticPropertySet {
                class_name: factory_call(),
                property: "count".to_string(),
                value: EvalExpr::Const(EvalConst::Int(2)),
            },
            EvalStmt::DynamicStaticPropertySet {
                class_name: factory_call(),
                property: "count".to_string(),
                value: EvalExpr::Binary {
                    op: EvalBinOp::Add,
                    left: Box::new(EvalExpr::DynamicStaticPropertyGet {
                        class_name: Box::new(factory_call()),
                        property: "count".to_string(),
                    }),
                    right: Box::new(EvalExpr::Const(EvalConst::Int(3))),
                },
            },
            EvalStmt::DynamicStaticPropertyArrayAppend {
                class_name: factory_call(),
                property: "items".to_string(),
                value: EvalExpr::Const(EvalConst::Int(4)),
            },
            EvalStmt::DynamicStaticPropertyArraySet {
                class_name: factory_call(),
                property: "items".to_string(),
                index: EvalExpr::Const(EvalConst::Int(0)),
                op: None,
                value: EvalExpr::Const(EvalConst::Int(5)),
            },
            EvalStmt::DynamicStaticPropertyIncDec {
                class_name: factory_call(),
                property: "count".to_string(),
                increment: true,
            },
            EvalStmt::DynamicStaticPropertyIncDec {
                class_name: factory_call(),
                property: "count".to_string(),
                increment: true,
            },
            EvalStmt::DynamicStaticPropertyIncDec {
                class_name: factory_call(),
                property: "count".to_string(),
                increment: false,
            },
        ]
    );
}
