//! Purpose:
//! Parser tests for class/member attributes and legacy `var` properties.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Attribute literal and constant-expression metadata and legacy modifier compatibility are
//!   covered.

use super::super::support::*;

/// Attribute construction after an array dimension remains a valid expression statement.
#[test]
fn parse_fragment_core_attribute_instance_expression_statement() {
    let program = parse_fragment(br#"try { $attributes[0]->newInstance(); } catch (Error $error) {}"#);
    assert!(program.is_ok(), "{program:?}");
}

/// Verifies class attributes lower to eval class metadata with supported literal args.
#[test]
fn parse_fragment_accepts_class_attribute_metadata() {
    let program = parse_fragment(
        br#"#[Route("/home", -1, 1.5, -2.5, true, null, EvalAttrDep::class, ["nested", "key" => 2])]
#[Tag(name: "named")]
class DynEvalAttributed {}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::class_decl(
            EvalClass::new("DynEvalAttributed", Vec::new(), Vec::new()).with_attributes(vec![
                EvalAttribute::new(
                    "Route",
                    Some(vec![
                        EvalAttributeArg::String("/home".to_string()),
                        EvalAttributeArg::Int(-1),
                        EvalAttributeArg::Float(1.5f64.to_bits()),
                        EvalAttributeArg::Float((-2.5f64).to_bits()),
                        EvalAttributeArg::Bool(true),
                        EvalAttributeArg::Null,
                        EvalAttributeArg::String("EvalAttrDep".to_string()),
                        EvalAttributeArg::Array(vec![
                            EvalAttributeArg::String("nested".to_string()),
                            EvalAttributeArg::Named {
                                name: "key".to_string(),
                                value: Box::new(EvalAttributeArg::Int(2)),
                            },
                        ]),
                    ]),
                ),
                EvalAttribute::new(
                    "Tag",
                    Some(vec![EvalAttributeArg::Named {
                        name: "name".to_string(),
                        value: Box::new(EvalAttributeArg::String("named".to_string())),
                    }]),
                ),
            ])
        )]
    );
}

/// Retains non-UTF-8 attribute literals as bytes instead of changing them or rejecting the metadata.
#[test]
fn string_literal_bytes_survive_attribute_metadata() {
    let program = parse_fragment(br#"#[Tag("\xFF")] class BinaryTag {}"#).unwrap();
    assert_eq!(program.statements(), &[EvalStmt::ClassDecl(std::sync::Arc::new(EvalClass::new("BinaryTag", Vec::new(), Vec::new())
        .with_attributes(vec![EvalAttribute::new("Tag", Some(vec![EvalAttributeArg::Bytes(vec![0xff])]))])))]);
}

/// Keeps a constant-expression attribute argument as an expression for reflection to evaluate,
/// while a non-constant one (a variable) still leaves the attribute's arguments unsupported.
#[test]
fn parse_fragment_keeps_constant_expression_attribute_args() {
    let program = parse_fragment(
        br#"#[Route("/x", requirements: ["page" => Requirement::POSITIVE_INT])]
#[Tag($dynamic)]
class ConstExprAttributed {}"#,
    )
    .expect("fragment should parse");
    let [EvalStmt::ClassDecl(class)] = program.statements() else {
        panic!("expected one class declaration");
    };
    let attributes = class.attributes();
    let route_args = attributes[0].args().expect("route args are supported");
    assert_eq!(route_args[0], EvalAttributeArg::String("/x".to_string()));
    let EvalAttributeArg::Named { name, value } = &route_args[1] else {
        panic!("expected a named argument");
    };
    assert_eq!(name, "requirements");
    assert!(matches!(value.as_ref(), EvalAttributeArg::Expr(_)));
    assert!(attributes[1].args().is_none());
}

/// Verifies class-like declaration attributes attach to interfaces, traits, and enums.
#[test]
fn parse_fragment_accepts_class_like_attribute_metadata() {
    let program = parse_fragment(
        br#"#[IfaceMark] interface DynEvalAttrIface {}
#[TraitMark] trait DynEvalAttrTrait {}
#[EnumMark] enum DynEvalAttrEnum { case Ready; }"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::InterfaceDecl(
                EvalInterface::new("DynEvalAttrIface", Vec::new(), Vec::new())
                    .with_attributes(vec![EvalAttribute::new("IfaceMark", Some(Vec::new()))])
            ),
            EvalStmt::TraitDecl(
                EvalTrait::new("DynEvalAttrTrait", Vec::new(), Vec::new())
                    .with_attributes(vec![EvalAttribute::new("TraitMark", Some(Vec::new()))])
            ),
            EvalStmt::EnumDecl(
                EvalEnum::new(
                    "DynEvalAttrEnum",
                    None,
                    vec![EvalEnumCase::new("Ready", None)]
                )
                .with_attributes(vec![EvalAttribute::new("EnumMark", Some(Vec::new()))])
            ),
        ]
    );
}

/// Verifies attributes on class constants, properties, and methods are retained.
#[test]
fn parse_fragment_accepts_class_member_attribute_metadata() {
    let program = parse_fragment(
        br#"class DynEvalMemberAttrs {
    #[ConstMark]
    public const SEED = 1;
    #[PropMark("p")]
    public int $value;
    #[MethodMark(true)]
    public function read() { return 1; }
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::class_decl(
            EvalClass::with_modifiers_traits_and_constants(
                "DynEvalMemberAttrs",
                false,
                false,
                None,
                Vec::new(),
                Vec::new(),
                vec![
                    EvalClassConstant::new("SEED", EvalExpr::Const(EvalConst::Int(1)))
                        .with_attributes(vec![EvalAttribute::new("ConstMark", Some(Vec::new()))])
                ],
                vec![EvalClassProperty::new("value", None)
                    .with_type(Some(EvalParameterType::new(
                        vec![EvalParameterTypeVariant::Int],
                        false
                    )))
                    .with_attributes(vec![EvalAttribute::new(
                        "PropMark",
                        Some(vec![EvalAttributeArg::String("p".to_string())])
                    )])],
                vec![EvalClassMethod::new(
                    "read",
                    Vec::new(),
                    vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::Int(1))))]
                )
                .with_attributes(vec![EvalAttribute::new(
                    "MethodMark",
                    Some(vec![EvalAttributeArg::Bool(true)])
                )])],
            )
        )]
    );
}

/// Verifies PHP's legacy `var` property marker parses as public property syntax.
#[test]
fn parse_fragment_accepts_legacy_var_properties() {
    let program = parse_fragment(
        br#"class DynEvalVarProps {
    var $plain = "p";
    var ?int $count = null;
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::class_decl(EvalClass::new(
            "DynEvalVarProps",
            vec![
                EvalClassProperty::new(
                    "plain",
                    Some(EvalExpr::Const(EvalConst::String("p".to_string())))
                ),
                EvalClassProperty::new("count", Some(EvalExpr::Const(EvalConst::Null)))
                    .with_type(Some(EvalParameterType::new(
                        vec![EvalParameterTypeVariant::Int],
                        true
                    )))
            ],
            Vec::new(),
        ))]
    );
}

/// Verifies legacy `var` property syntax also works inside eval traits.
#[test]
fn parse_fragment_accepts_legacy_var_trait_properties() {
    let program = parse_fragment(
        br#"trait DynEvalVarTrait {
    var ?int $count = null;
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::TraitDecl(EvalTrait::new(
            "DynEvalVarTrait",
            vec![
                EvalClassProperty::new("count", Some(EvalExpr::Const(EvalConst::Null)))
                    .with_type(Some(EvalParameterType::new(
                        vec![EvalParameterTypeVariant::Int],
                        true
                    )))
            ],
            Vec::new(),
        ))]
    );
}

/// Verifies legacy `var` cannot be combined with other property modifiers.
#[test]
fn parse_fragment_rejects_legacy_var_modifier_combinations() {
    for source in [
        b"class DynEvalBadPublicVar { public var $value; }" as &[u8],
        b"class DynEvalBadStaticVar { static var $value; }",
        b"class DynEvalBadReadonlyVar { readonly var $value; }",
    ] {
        assert_eq!(
            parse_fragment_error(source),
            Err(EvalParseError::UnsupportedConstruct)
        );
    }
}

/// Verifies interface, trait, and enum member attributes are retained.
#[test]
fn parse_fragment_accepts_class_like_member_attribute_metadata() {
    let program = parse_fragment(
        br#"interface DynEvalMemberIface {
    #[IfaceProp]
    public string $value { get; }
    #[IfaceMethod]
    function read();
}
trait DynEvalMemberTrait {
    #[TraitProp]
    public int $seed;
    #[TraitMethod]
    public function add() { return 2; }
}
enum DynEvalMemberEnum {
    #[CaseMark]
    case Ready;
    #[EnumMethod]
    public function label() { return "ready"; }
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::InterfaceDecl(EvalInterface::with_constants_and_properties(
                "DynEvalMemberIface",
                Vec::new(),
                Vec::new(),
                vec![EvalInterfaceProperty::new("value", true, false)
                    .with_type(Some(EvalParameterType::new(
                        vec![EvalParameterTypeVariant::String],
                        false
                    )))
                    .with_attributes(vec![EvalAttribute::new("IfaceProp", Some(Vec::new()))])],
                vec![EvalInterfaceMethod::new("read", Vec::new())
                    .with_attributes(vec![EvalAttribute::new("IfaceMethod", Some(Vec::new()))])],
            )),
            EvalStmt::TraitDecl(EvalTrait::new(
                "DynEvalMemberTrait",
                vec![EvalClassProperty::new("seed", None)
                    .with_type(Some(EvalParameterType::new(
                        vec![EvalParameterTypeVariant::Int],
                        false
                    )))
                    .with_attributes(vec![EvalAttribute::new("TraitProp", Some(Vec::new()))])],
                vec![EvalClassMethod::new(
                    "add",
                    Vec::new(),
                    vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::Int(2))))]
                )
                .with_attributes(vec![EvalAttribute::new("TraitMethod", Some(Vec::new()))])],
            )),
            EvalStmt::EnumDecl(EvalEnum::with_members(
                "DynEvalMemberEnum",
                None,
                Vec::new(),
                vec![EvalEnumCase::new("Ready", None)
                    .with_attributes(vec![EvalAttribute::new("CaseMark", Some(Vec::new()))])],
                Vec::new(),
                vec![EvalClassMethod::new(
                    "label",
                    Vec::new(),
                    vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::String(
                        "ready".to_string()
                    ))))]
                )
                .with_attributes(vec![EvalAttribute::new("EnumMethod", Some(Vec::new()))])],
            )),
        ]
    );
}

/// Verifies an attribute argument list may end with a trailing comma.
///
/// It is an ordinary argument list in PHP, and a multi-line `#[AsCommand(name: …, description: …,)]`
/// is how `symfony/error-handler/Command/ErrorDumpCommand.php` is written; the parser named the
/// closing parenthesis as the unexpected token.
#[test]
fn parse_fragment_accepts_a_trailing_comma_in_an_attribute_argument_list() {
    let program = parse_fragment(
        b"#[AsCommand(\n    name: \"error:dump\",\n)]\nfinal class DynEvalTrailingCommaAttr {}",
    )
    .expect("fragment should parse");
    let [EvalStmt::ClassDecl(class)] = program.statements() else {
        panic!(
            "expected one class declaration, got {:?}",
            program.statements()
        );
    };
    assert_eq!(
        class.attributes(),
        &[EvalAttribute::new(
            "AsCommand",
            Some(vec![EvalAttributeArg::Named {
                name: "name".to_string(),
                value: Box::new(EvalAttributeArg::String("error:dump".to_string())),
            }])
        )]
    );
}
