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
    final public const SEED = 2;
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
                    EvalClassConstant::with_visibility_and_final(
                        "SEED",
                        EvalVisibility::Public,
                        true,
                        EvalExpr::Const(EvalConst::Int(2)),
                    ),
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

/// Verifies `ClassName::class` lowers to a class-name fetch rather than a user constant.
#[test]
fn parse_fragment_accepts_class_name_fetches() {
    let program = parse_fragment(br#"return EvalConstBox::class;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::ClassNameFetch {
            class_name: "EvalConstBox".to_string(),
        }))]
    );
}

/// Verifies interface constants lower into eval interface metadata.
#[test]
fn parse_fragment_accepts_interface_constant_declarations() {
    let program = parse_fragment(
        br#"interface EvalConstIface {
    final public const SEED = 4;
    function read();
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::InterfaceDecl(EvalInterface::with_constants(
            "EvalConstIface",
            Vec::new(),
            vec![EvalClassConstant::with_visibility_and_final(
                "SEED",
                EvalVisibility::Public,
                true,
                EvalExpr::Const(EvalConst::Int(4)),
            )],
            vec![EvalInterfaceMethod::new("read", Vec::new())],
        ))]
    );
}

/// Verifies trait constants lower into eval trait metadata.
#[test]
fn parse_fragment_accepts_trait_constant_declarations() {
    let program = parse_fragment(
        br#"trait EvalConstTrait {
    final public const SEED = 6;
    public function read() { return self::SEED; }
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::TraitDecl(EvalTrait::with_constants(
            "EvalConstTrait",
            vec![EvalClassConstant::with_visibility_and_final(
                "SEED",
                EvalVisibility::Public,
                true,
                EvalExpr::Const(EvalConst::Int(6)),
            )],
            Vec::new(),
            vec![EvalClassMethod::new(
                "read",
                Vec::new(),
                vec![EvalStmt::Return(Some(EvalExpr::ClassConstantFetch {
                    class_name: "self".to_string(),
                    constant: "SEED".to_string(),
                }))],
            )],
        ))]
    );
}
