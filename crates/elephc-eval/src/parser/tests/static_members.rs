//! Purpose:
//! Parser tests for eval static property and static method syntax.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
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
