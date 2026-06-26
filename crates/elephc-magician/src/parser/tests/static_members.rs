//! Purpose:
//! Parser tests for eval static property and static method syntax.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases verify `::` receivers lower to EvalIR static-member nodes.

use super::support::*;

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
