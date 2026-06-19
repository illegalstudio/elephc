//! Purpose:
//! Parser tests for class declarations and parser diagnostics.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases cover dynamic class metadata and malformed fragment errors.

use super::support::*;

/// Verifies empty class declarations lower to dynamic class-registration statements.
#[test]
fn parse_fragment_accepts_empty_class_declaration_source() {
    let program = parse_fragment(b"class DynEvalClass {};").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::new(
            "DynEvalClass",
            Vec::new(),
            Vec::new()
        ))]
    );
}
/// Verifies class relation clauses lower into dynamic class metadata.
#[test]
fn parse_fragment_accepts_class_extends_and_implements_source() {
    let program = parse_fragment(
        br#"class DynEvalChild extends DynEvalBase implements DynEvalIface, \Root\Iface {}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::with_relations(
            "DynEvalChild",
            Some("DynEvalBase".to_string()),
            vec!["DynEvalIface".to_string(), "Root\\Iface".to_string()],
            Vec::new(),
            Vec::new(),
        ))]
    );
}

/// Verifies class attributes lower to eval class metadata with supported literal args.
#[test]
fn parse_fragment_accepts_class_attribute_metadata() {
    let program = parse_fragment(
        br#"#[Route("/home", -1, true, null)]
#[Tag(name: "named")]
class DynEvalAttributed {}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(
            EvalClass::new("DynEvalAttributed", Vec::new(), Vec::new()).with_attributes(vec![
                EvalAttribute::new(
                    "Route",
                    Some(vec![
                        EvalAttributeArg::String("/home".to_string()),
                        EvalAttributeArg::Int(-1),
                        EvalAttributeArg::Bool(true),
                        EvalAttributeArg::Null,
                    ]),
                ),
                EvalAttribute::new("Tag", None),
            ])
        )]
    );
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
        &[EvalStmt::ClassDecl(
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
                vec![EvalClassProperty::new("value", None).with_attributes(vec![
                    EvalAttribute::new(
                        "PropMark",
                        Some(vec![EvalAttributeArg::String("p".to_string())])
                    )
                ])],
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
                    .with_attributes(vec![EvalAttribute::new("IfaceProp", Some(Vec::new()))])],
                vec![EvalInterfaceMethod::new("read", Vec::new())
                    .with_attributes(vec![EvalAttribute::new("IfaceMethod", Some(Vec::new()))])],
            )),
            EvalStmt::TraitDecl(EvalTrait::new(
                "DynEvalMemberTrait",
                vec![EvalClassProperty::new("seed", None)
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

/// Verifies eval interface declarations lower to dynamic interface metadata.
#[test]
fn parse_fragment_accepts_interface_declaration_source() {
    let program = parse_fragment(
        br#"interface DynEvalIface extends ParentIface, \Root\Iface {
    public function read($value);
    function label();
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::InterfaceDecl(EvalInterface::new(
            "DynEvalIface",
            vec!["ParentIface".to_string(), "Root\\Iface".to_string()],
            vec![
                EvalInterfaceMethod::new("read", vec!["value".to_string()]),
                EvalInterfaceMethod::new("label", Vec::new()),
            ],
        ))]
    );
}

/// Verifies interface property hook contracts lower to eval interface metadata.
#[test]
fn parse_fragment_accepts_interface_property_hook_contracts() {
    let program = parse_fragment(
        br#"interface DynEvalHookIface {
    public string $value { get; set; }
    public int $id { get; }
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::InterfaceDecl(
            EvalInterface::with_constants_and_properties(
                "DynEvalHookIface",
                Vec::new(),
                Vec::new(),
                vec![
                    EvalInterfaceProperty::new("value", true, true),
                    EvalInterfaceProperty::new("id", true, false),
                ],
                Vec::new(),
            )
        )]
    );
}

/// Verifies public property and method class members lower into dynamic class metadata.
#[test]
fn parse_fragment_accepts_public_class_members() {
    let program = parse_fragment(
            b"class DynEvalSupported { public int $x = 1; public function read() { return $this->x; } }",
        )
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::new(
            "DynEvalSupported",
            vec![EvalClassProperty::new(
                "x",
                Some(EvalExpr::Const(EvalConst::Int(1)))
            )],
            vec![EvalClassMethod::new(
                "read",
                Vec::new(),
                vec![EvalStmt::Return(Some(EvalExpr::PropertyGet {
                    object: Box::new(EvalExpr::LoadVar("this".to_string())),
                    property: "x".to_string(),
                }))]
            )]
        ))]
    );
}
/// Verifies private and protected class members lower with explicit visibility metadata.
#[test]
fn parse_fragment_accepts_private_and_protected_class_members() {
    let program = parse_fragment(
        b"class DynEvalVisibility { private int $secret = 3; protected function reveal() { return $this->secret; } }",
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::new(
            "DynEvalVisibility",
            vec![EvalClassProperty::with_visibility(
                "secret",
                EvalVisibility::Private,
                Some(EvalExpr::Const(EvalConst::Int(3)))
            )],
            vec![EvalClassMethod::with_visibility_and_modifiers(
                "reveal",
                EvalVisibility::Protected,
                false,
                false,
                false,
                Vec::new(),
                vec![EvalStmt::Return(Some(EvalExpr::PropertyGet {
                    object: Box::new(EvalExpr::LoadVar("this".to_string())),
                    property: "secret".to_string(),
                }))]
            )]
        ))]
    );
}

/// Verifies readonly property modifiers lower into dynamic class metadata.
#[test]
fn parse_fragment_accepts_readonly_class_property() {
    let program = parse_fragment(b"class DynEvalReadonly { public readonly int $id; }")
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::new(
            "DynEvalReadonly",
            vec![EvalClassProperty::with_visibility_static_and_readonly(
                "id",
                EvalVisibility::Public,
                false,
                true,
                None
            )],
            Vec::new()
        ))]
    );
}

/// Verifies readonly class modifiers lower into class and property metadata.
#[test]
fn parse_fragment_accepts_readonly_class_modifier() {
    let program = parse_fragment(
        b"final readonly class DynEvalReadonlyClass { public int $id; public static int $count = 0; }",
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::with_class_modifiers(
            "DynEvalReadonlyClass",
            false,
            true,
            true,
            None,
            Vec::new(),
            vec![
                EvalClassProperty::with_visibility_static_and_readonly(
                    "id",
                    EvalVisibility::Public,
                    false,
                    true,
                    None
                ),
                EvalClassProperty::with_visibility_static_and_readonly(
                    "count",
                    EvalVisibility::Public,
                    true,
                    false,
                    Some(EvalExpr::Const(EvalConst::Int(0)))
                )
            ],
            Vec::new()
        ))]
    );
}

/// Verifies concrete property hooks lower to property metadata plus accessor methods.
#[test]
fn parse_fragment_accepts_concrete_class_property_hooks() {
    let program = parse_fragment(
        br#"class DynEvalHooked {
    public int $value {
        get => 7;
        set { return; }
    }
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::new(
            "DynEvalHooked",
            vec![EvalClassProperty::with_visibility_static_and_readonly(
                "value",
                EvalVisibility::Public,
                false,
                false,
                None
            )
            .with_hooks(true, true)],
            vec![
                EvalClassMethod::new(
                    "__propget_value",
                    Vec::new(),
                    vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::Int(7))))]
                ),
                EvalClassMethod::new(
                    "__propset_value",
                    vec!["value".to_string()],
                    vec![EvalStmt::Return(None)]
                )
            ]
        ))]
    );
}

/// Verifies abstract property hook contracts lower without concrete accessors.
#[test]
fn parse_fragment_accepts_abstract_class_property_hook_contracts() {
    let program = parse_fragment(
        br#"abstract class DynEvalAbstractHooked {
    abstract public int $value { get; set; }
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::with_modifiers(
            "DynEvalAbstractHooked",
            true,
            false,
            None,
            Vec::new(),
            vec![EvalClassProperty::with_visibility_static_and_readonly(
                "value",
                EvalVisibility::Public,
                false,
                false,
                None
            )
            .with_abstract_hook_contract(true, true)],
            Vec::new(),
        ))]
    );
}

/// Verifies trait abstract property hook contracts lower without concrete accessors.
#[test]
fn parse_fragment_accepts_trait_abstract_property_hook_contracts() {
    let program = parse_fragment(
        br#"trait DynEvalAbstractHookTrait {
    abstract protected string $name { get; }
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::TraitDecl(EvalTrait::new(
            "DynEvalAbstractHookTrait",
            vec![EvalClassProperty::with_visibility_static_and_readonly(
                "name",
                EvalVisibility::Protected,
                false,
                false,
                None
            )
            .with_abstract_hook_contract(true, false)],
            Vec::new(),
        ))]
    );
}

/// Verifies eval rejects readonly property forms that PHP does not allow.
#[test]
fn parse_fragment_rejects_invalid_readonly_class_properties() {
    parse_fragment(b"class DynEvalReadonlyDefault { public readonly int $id = 1; }")
        .expect_err("readonly properties cannot have defaults in eval");
    parse_fragment(b"class DynEvalReadonlyStatic { public static readonly int $id; }")
        .expect_err("static properties cannot be readonly in eval");
    parse_fragment(b"readonly class DynEvalReadonlyClassDefault { public int $id = 1; }")
        .expect_err("readonly class instance properties cannot have defaults in eval");
}

/// Verifies eval rejects property hook forms that need broader class contracts.
#[test]
fn parse_fragment_rejects_invalid_property_hooks() {
    parse_fragment(b"class DynEvalHookDefault { public int $id = 1 { get => $this->id; } }")
        .expect_err("hooked properties cannot have defaults in eval");
    parse_fragment(b"class DynEvalHookStatic { public static int $id { get => 1; } }")
        .expect_err("static properties cannot have hooks in eval");
    parse_fragment(
        b"abstract class DynEvalHookAbstractDefault { abstract public int $id = 1 { get; } }",
    )
    .expect_err("abstract properties cannot have defaults in eval");
    parse_fragment(
        b"abstract class DynEvalHookAbstractStatic { abstract public static int $id { get; } }",
    )
    .expect_err("static properties cannot have abstract hooks in eval");
}

/// Verifies eval rejects concrete hook bodies in interface property contracts.
#[test]
fn parse_fragment_rejects_invalid_interface_property_hooks() {
    parse_fragment(b"interface DynEvalIfaceHookBody { public int $id { get => 1; } }")
        .expect_err("interface property hooks cannot have concrete bodies");
    parse_fragment(b"interface DynEvalIfaceHookDuplicate { public int $id { get; get; } }")
        .expect_err("interface property hooks cannot repeat contracts");
    parse_fragment(b"interface DynEvalIfaceHookEmpty { public int $id { } }")
        .expect_err("interface property hooks require at least one contract");
    parse_fragment(b"interface DynEvalIfaceHookDefault { public int $id = 1 { get; } }")
        .expect_err("interface property hook contracts cannot have defaults");
}

/// Verifies abstract and final class modifiers lower into dynamic class metadata.
#[test]
fn parse_fragment_accepts_abstract_and_final_class_members() {
    let program = parse_fragment(
        br#"abstract class DynEvalAbstract {
    abstract public function read($value);
    final public function label() { return "base"; }
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::with_modifiers(
            "DynEvalAbstract",
            true,
            false,
            None,
            Vec::new(),
            Vec::new(),
            vec![
                EvalClassMethod::with_modifiers(
                    "read",
                    true,
                    false,
                    vec!["value".to_string()],
                    Vec::new()
                ),
                EvalClassMethod::with_modifiers(
                    "label",
                    false,
                    true,
                    Vec::new(),
                    vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::String(
                        "base".to_string()
                    ))))]
                ),
            ],
        ))]
    );
}

/// Verifies eval method parameters retain declared-type presence for reflection metadata.
#[test]
fn parse_fragment_accepts_typed_method_parameter_metadata() {
    let program = parse_fragment(
        br#"class DynEvalTypedParams {
    public function run(int $id, ?\App\Name $name, string|null $label) {}
}"#,
    )
    .expect("fragment should parse");
    let [EvalStmt::ClassDecl(class)] = program.statements() else {
        panic!("expected one class declaration");
    };
    let [method] = class.methods() else {
        panic!("expected one class method");
    };
    assert_eq!(
        method.params(),
        &["id".to_string(), "name".to_string(), "label".to_string()]
    );
    assert_eq!(method.parameter_has_types(), &[true, true, true]);
}

/// Verifies trait declarations and class trait uses lower into dynamic metadata.
#[test]
fn parse_fragment_accepts_trait_declaration_and_class_use() {
    let program = parse_fragment(
        br#"trait DynEvalTrait {
    public int $seed = 2;
    public function read($value) { return $this->seed + $value; }
}
class DynEvalUsesTrait {
    use DynEvalTrait, \Root\SharedTrait;
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::TraitDecl(EvalTrait::new(
                "DynEvalTrait",
                vec![EvalClassProperty::new(
                    "seed",
                    Some(EvalExpr::Const(EvalConst::Int(2)))
                )],
                vec![EvalClassMethod::new(
                    "read",
                    vec!["value".to_string()],
                    vec![EvalStmt::Return(Some(EvalExpr::Binary {
                        op: EvalBinOp::Add,
                        left: Box::new(EvalExpr::PropertyGet {
                            object: Box::new(EvalExpr::LoadVar("this".to_string())),
                            property: "seed".to_string(),
                        }),
                        right: Box::new(EvalExpr::LoadVar("value".to_string())),
                    }))]
                )],
            )),
            EvalStmt::ClassDecl(EvalClass::with_modifiers_and_traits(
                "DynEvalUsesTrait",
                false,
                false,
                None,
                Vec::new(),
                vec!["DynEvalTrait".to_string(), "Root\\SharedTrait".to_string()],
                Vec::new(),
                Vec::new(),
            )),
        ]
    );
}
/// Verifies malformed object construction reports an unexpected token.
#[test]
fn parse_fragment_rejects_new_without_class_name() {
    assert_eq!(
        parse_fragment(b"return new ();"),
        Err(EvalParseError::UnexpectedToken)
    );
}
/// Verifies unsupported expression keywords report the unsupported construct status.
#[test]
fn parse_fragment_rejects_expression_keywords_as_unsupported_constructs() {
    for source in [
        b"return clone $value;" as &[u8],
        b"return yield 1;" as &[u8],
    ] {
        assert_eq!(
            parse_fragment(source),
            Err(EvalParseError::UnsupportedConstruct)
        );
    }
}
/// Verifies malformed statements report parse errors instead of partial IR.
#[test]
fn parse_fragment_rejects_missing_semicolon() {
    assert_eq!(
        parse_fragment(b"$x = 1"),
        Err(EvalParseError::ExpectedSemicolon)
    );
}
