//! Purpose:
//! Parser tests for eval-declared pure and backed enum declarations.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover enum cases, backing types, interfaces, constants, and methods.

use super::support::*;

/// Verifies pure enum cases lower to dynamic enum metadata.
#[test]
fn parse_fragment_accepts_pure_enum_declaration_source() {
    let program =
        parse_fragment(b"enum EvalSuit { case Hearts; case Clubs; }").expect("parse eval enum");
    assert_eq!(
        program.statements(),
        &[EvalStmt::EnumDecl(EvalEnum::new(
            "EvalSuit",
            None,
            vec![
                EvalEnumCase::new("Hearts", None),
                EvalEnumCase::new("Clubs", None),
            ],
        ))]
    );
}

/// Verifies backed enum metadata preserves interfaces, case values, constants, and methods.
#[test]
fn parse_fragment_accepts_backed_enum_members() {
    let program = parse_fragment(
        br#"enum EvalColor: string implements EvalLabel {
    case Red = "r";
    final public const PREFIX = "color";
    public function label() { return self::PREFIX . ":" . $this->name; }
}"#,
    )
    .expect("parse backed eval enum");
    assert_eq!(
        program.statements(),
        &[EvalStmt::EnumDecl(EvalEnum::with_members(
            "EvalColor",
            Some(EvalEnumBackingType::String),
            vec!["EvalLabel".to_string()],
            vec![EvalEnumCase::new(
                "Red",
                Some(EvalExpr::Const(EvalConst::String("r".to_string()))),
            )],
            vec![EvalClassConstant::with_visibility_and_final(
                "PREFIX",
                EvalVisibility::Public,
                true,
                EvalExpr::Const(EvalConst::String("color".to_string())),
            )],
            vec![EvalClassMethod::new(
                "label",
                Vec::new(),
                vec![EvalStmt::Return(Some(EvalExpr::Binary {
                    op: EvalBinOp::Concat,
                    left: Box::new(EvalExpr::Binary {
                        op: EvalBinOp::Concat,
                        left: Box::new(EvalExpr::ClassConstantFetch {
                            class_name: "self".to_string(),
                            constant: "PREFIX".to_string(),
                        }),
                        right: Box::new(EvalExpr::Const(EvalConst::String(":".to_string()))),
                    }),
                    right: Box::new(EvalExpr::PropertyGet {
                        object: Box::new(EvalExpr::LoadVar("this".to_string())),
                        property: "name".to_string(),
                    }),
                }))],
            )],
        ))]
    );
}

/// Verifies enum declarations retain trait uses and adaptations.
#[test]
fn parse_fragment_accepts_enum_trait_use() {
    let program = parse_fragment(
        br#"enum EvalTraitEnum {
    use EvalEnumTrait {
        label as private hiddenLabel;
    }
    case Ready;
}"#,
    )
    .expect("parse eval enum with trait use");

    let [EvalStmt::EnumDecl(enum_decl)] = program.statements() else {
        panic!("fragment should contain one enum declaration");
    };
    assert_eq!(enum_decl.traits(), &["EvalEnumTrait".to_string()]);
    assert_eq!(
        enum_decl.trait_adaptations(),
        &[EvalTraitAdaptation::Alias {
            trait_name: None,
            method: "label".to_string(),
            alias: Some("hiddenLabel".to_string()),
            visibility: Some(EvalVisibility::Private),
        }]
    );
}
