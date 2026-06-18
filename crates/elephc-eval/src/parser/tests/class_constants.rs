//! Purpose:
//! Parser tests for eval class constant declarations and fetches.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases verify `const` members and `Class::CONST` lower to EvalIR.

use super::support::*;

/// Verifies class constants lower into eval class metadata.
#[test]
fn parse_fragment_accepts_class_constant_declarations() {
    let program = parse_fragment(
        br#"class EvalConstBox {
    public const SEED = 2;
    protected const LABEL = "box";
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(
            EvalClass::with_modifiers_traits_and_constants(
                "EvalConstBox",
                false,
                false,
                None,
                Vec::new(),
                Vec::new(),
                vec![
                    EvalClassConstant::new("SEED", EvalExpr::Const(EvalConst::Int(2))),
                    EvalClassConstant::with_visibility(
                        "LABEL",
                        EvalVisibility::Protected,
                        EvalExpr::Const(EvalConst::String("box".to_string())),
                    ),
                ],
                Vec::new(),
                Vec::new(),
            )
        )]
    );
}

/// Verifies class constant fetches lower to EvalIR expressions.
#[test]
fn parse_fragment_accepts_class_constant_fetches() {
    let program = parse_fragment(br#"return EvalConstBox::SEED;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::ClassConstantFetch {
            class_name: "EvalConstBox".to_string(),
            constant: "SEED".to_string(),
        }))]
    );
}

/// Verifies scoped class constant fetches preserve the class-like receiver.
#[test]
fn parse_fragment_accepts_scoped_class_constant_fetches() {
    let program = parse_fragment(br#"return self::SEED + parent::SEED + static::SEED;"#)
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Add,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::ClassConstantFetch {
                    class_name: "self".to_string(),
                    constant: "SEED".to_string(),
                }),
                right: Box::new(EvalExpr::ClassConstantFetch {
                    class_name: "parent".to_string(),
                    constant: "SEED".to_string(),
                }),
            }),
            right: Box::new(EvalExpr::ClassConstantFetch {
                class_name: "static".to_string(),
                constant: "SEED".to_string(),
            }),
        }))]
    );
}
