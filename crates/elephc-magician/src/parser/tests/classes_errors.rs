//! Purpose:
//! Parser tests for class declarations and parser diagnostics.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
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

/// Verifies function, interface, and class method return types are retained.
#[test]
fn parse_fragment_accepts_return_type_metadata() {
    let program = parse_fragment(
        br#"function DynEvalReturn(): ?int { return 1; }
interface DynEvalReturnIface {
    public function read(): string;
}
class DynEvalReturnClass {
    public function selfReturn(): static { return $this; }
    public function done(): void {}
}"#,
    )
    .expect("fragment should parse");
    let statements = program.statements();
    let EvalStmt::FunctionDecl {
        return_type: Some(function_return_type),
        ..
    } = &statements[0]
    else {
        panic!("expected function declaration with return type");
    };
    assert_eq!(
        function_return_type.variants(),
        &[EvalParameterTypeVariant::Int]
    );
    assert!(function_return_type.allows_null());

    let EvalStmt::InterfaceDecl(interface) = &statements[1] else {
        panic!("expected interface declaration");
    };
    let interface_return_type = interface.methods()[0]
        .return_type()
        .expect("interface method return type");
    assert_eq!(
        interface_return_type.variants(),
        &[EvalParameterTypeVariant::String]
    );

    let EvalStmt::ClassDecl(class) = &statements[2] else {
        panic!("expected class declaration");
    };
    let self_return_type = class.methods()[0]
        .return_type()
        .expect("class method return type");
    assert_eq!(
        self_return_type.variants(),
        &[EvalParameterTypeVariant::Class("static".to_string())]
    );
    let void_return_type = class.methods()[1]
        .return_type()
        .expect("void method return type");
    assert_eq!(
        void_return_type.variants(),
        &[EvalParameterTypeVariant::Void]
    );
}

/// Verifies type atoms are rejected in positions where PHP forbids them.
#[test]
fn parse_fragment_rejects_invalid_type_atom_forms() {
    for source in [
        b"function DynEvalBadVoid(): ?void {}" as &[u8],
        b"function DynEvalBadVoidUnion(): void|null {}",
        b"function DynEvalBadNeverUnion(): never|int {}",
        b"function DynEvalBadNeverIntersection(): never&Countable {}",
        b"function DynEvalBadVoidParam(void $value) {}",
        b"function DynEvalBadSelfParam(self $value) {}",
        b"function DynEvalBadParentParam(parent $value) {}",
        b"function DynEvalBadStaticParam(static $value) {}",
        b"function DynEvalBadSelfReturn(): self {}",
        b"function DynEvalBadParentReturn(): parent {}",
        b"function DynEvalBadStaticReturn(): static {}",
        b"class DynEvalBadStaticMethodParam { public function read(static $value) {} }",
        b"class DynEvalBadCallableProperty { public callable $value; }",
        b"class DynEvalBadStaticProperty { public static static $value; }",
        b"interface DynEvalBadCallableInterfaceProperty { public callable $value { get; } }",
        b"class DynEvalBadCallablePromoted { public function __construct(public callable $value) {} }",
        b"class DynEvalBadStaticPromoted { public function __construct(public static $value) {} }",
    ] {
        assert_eq!(
            parse_fragment(source),
            Err(EvalParseError::UnsupportedConstruct)
        );
    }
}

/// Verifies class attributes lower to eval class metadata with supported literal args.
#[test]
fn parse_fragment_accepts_class_attribute_metadata() {
    let program = parse_fragment(
        br#"#[Route("/home", -1, 1.5, -2.5, true, null, EvalAttrDep::class, ["nested", 2])]
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
                        EvalAttributeArg::Float(1.5f64.to_bits()),
                        EvalAttributeArg::Float((-2.5f64).to_bits()),
                        EvalAttributeArg::Bool(true),
                        EvalAttributeArg::Null,
                        EvalAttributeArg::String("EvalAttrDep".to_string()),
                        EvalAttributeArg::Array(vec![
                            EvalAttributeArg::String("nested".to_string()),
                            EvalAttributeArg::Int(2),
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
                    EvalInterfaceProperty::new("value", true, true).with_type(Some(
                        EvalParameterType::new(vec![EvalParameterTypeVariant::String], false)
                    )),
                    EvalInterfaceProperty::new("id", true, false).with_type(Some(
                        EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)
                    )),
                ],
                Vec::new(),
            )
        )]
    );
}

/// Verifies interface property contracts retain asymmetric set visibility.
#[test]
fn parse_fragment_accepts_interface_asymmetric_property_contracts() {
    let program = parse_fragment(
        br#"interface DynEvalAsymmetricIface {
    public protected(set) string $name { get; set; }
    private(set) int $id { get; set; }
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::InterfaceDecl(
            EvalInterface::with_constants_and_properties(
                "DynEvalAsymmetricIface",
                Vec::new(),
                Vec::new(),
                vec![
                    EvalInterfaceProperty::new("name", true, true)
                        .with_type(Some(EvalParameterType::new(
                            vec![EvalParameterTypeVariant::String],
                            false
                        )))
                        .with_set_visibility(Some(EvalVisibility::Protected)),
                    EvalInterfaceProperty::new("id", true, true)
                        .with_type(Some(EvalParameterType::new(
                            vec![EvalParameterTypeVariant::Int],
                            false
                        )))
                        .with_set_visibility(Some(EvalVisibility::Private)),
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
            vec![
                EvalClassProperty::new("x", Some(EvalExpr::Const(EvalConst::Int(1)))).with_type(
                    Some(EvalParameterType::new(
                        vec![EvalParameterTypeVariant::Int],
                        false
                    ))
                )
            ],
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

/// Verifies constructor-promoted properties lower to property metadata and assignments.
#[test]
fn parse_fragment_accepts_constructor_promoted_properties() {
    let program = parse_fragment(
        br#"class DynEvalPromoted {
    public function __construct(public int $id, private readonly ?string $name = null) {}
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::new(
            "DynEvalPromoted",
            vec![
                EvalClassProperty::with_visibility_static_final_and_readonly(
                    "id",
                    EvalVisibility::Public,
                    false,
                    false,
                    false,
                    None,
                )
                .with_type(Some(EvalParameterType::new(
                    vec![EvalParameterTypeVariant::Int],
                    false
                )))
                .with_promoted(),
                EvalClassProperty::with_visibility_static_final_and_readonly(
                    "name",
                    EvalVisibility::Private,
                    false,
                    false,
                    true,
                    None,
                )
                .with_type(Some(EvalParameterType::new(
                    vec![EvalParameterTypeVariant::String],
                    true
                )))
                .with_promoted(),
            ],
            vec![EvalClassMethod::new(
                "__construct",
                vec!["id".to_string(), "name".to_string()],
                vec![
                    EvalStmt::PropertySet {
                        object: EvalExpr::LoadVar("this".to_string()),
                        property: "id".to_string(),
                        value: EvalExpr::LoadVar("id".to_string()),
                    },
                    EvalStmt::PropertySet {
                        object: EvalExpr::LoadVar("this".to_string()),
                        property: "name".to_string(),
                        value: EvalExpr::LoadVar("name".to_string()),
                    },
                ],
            )
            .with_parameter_types(vec![
                Some(EvalParameterType::new(
                    vec![EvalParameterTypeVariant::Int],
                    false
                )),
                Some(EvalParameterType::new(
                    vec![EvalParameterTypeVariant::String],
                    true
                )),
            ])
            .with_parameter_defaults(vec![None, Some(EvalExpr::Const(EvalConst::Null))])]
        ))]
    );
}

/// Verifies by-reference promoted constructor parameters lower to property aliases.
#[test]
fn parse_fragment_accepts_by_reference_constructor_promoted_properties() {
    let program = parse_fragment(
        br#"class DynEvalPromotedRef {
    public function __construct(public int &$id) {}
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::new(
            "DynEvalPromotedRef",
            vec![
                EvalClassProperty::with_visibility_static_final_and_readonly(
                    "id",
                    EvalVisibility::Public,
                    false,
                    false,
                    false,
                    None,
                )
                .with_type(Some(EvalParameterType::new(
                    vec![EvalParameterTypeVariant::Int],
                    false
                )))
                .with_promoted()
            ],
            vec![EvalClassMethod::new(
                "__construct",
                vec!["id".to_string()],
                vec![EvalStmt::PropertyReferenceBind {
                    object: EvalExpr::LoadVar("this".to_string()),
                    property: "id".to_string(),
                    source: "id".to_string(),
                }],
            )
            .with_parameter_types(vec![Some(EvalParameterType::new(
                vec![EvalParameterTypeVariant::Int],
                false
            ))])
            .with_parameter_by_ref_flags(vec![true])]
        ))]
    );
}

/// Verifies eval rejects promoted parameter forms that the eval runtime cannot model yet.
#[test]
fn parse_fragment_rejects_unsupported_constructor_promotion_forms() {
    parse_fragment(b"class DynEvalPromotedMethod { public function run(public int $id) {} }")
        .expect_err("promotion is only valid on constructors");
    parse_fragment(
        b"class DynEvalPromotedVariadic { public function __construct(public ...$ids) {} }",
    )
    .expect_err("promoted variadic parameters need variadic property semantics");
    parse_fragment(
        b"interface DynEvalPromotedIface { public function __construct(public int $id); }",
    )
    .expect_err("interface signatures cannot promote properties");
    parse_fragment(b"enum DynEvalPromotedEnum { public function __construct(public int $id) {} }")
        .expect_err("enum methods cannot promote properties");
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
            )
            .with_type(Some(EvalParameterType::new(
                vec![EvalParameterTypeVariant::Int],
                false
            )))],
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
            )
            .with_type(Some(EvalParameterType::new(
                vec![EvalParameterTypeVariant::Int],
                false
            )))],
            Vec::new()
        ))]
    );
}

/// Verifies asymmetric property visibility lowers into eval class metadata.
#[test]
fn parse_fragment_accepts_asymmetric_property_visibility() {
    let program = parse_fragment(
        b"class DynEvalAsymmetric { public private(set) int $id = 1; protected(set) string $name = \"x\"; }",
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::new(
            "DynEvalAsymmetric",
            vec![
                EvalClassProperty::with_visibility_static_final_and_readonly(
                    "id",
                    EvalVisibility::Public,
                    false,
                    false,
                    false,
                    Some(EvalExpr::Const(EvalConst::Int(1)))
                )
                .with_type(Some(EvalParameterType::new(
                    vec![EvalParameterTypeVariant::Int],
                    false
                )))
                .with_set_visibility(Some(EvalVisibility::Private)),
                EvalClassProperty::with_visibility_static_final_and_readonly(
                    "name",
                    EvalVisibility::Public,
                    false,
                    false,
                    false,
                    Some(EvalExpr::Const(EvalConst::String("x".to_string())))
                )
                .with_type(Some(EvalParameterType::new(
                    vec![EvalParameterTypeVariant::String],
                    false
                )))
                .with_set_visibility(Some(EvalVisibility::Protected)),
            ],
            Vec::new()
        ))]
    );
}

/// Verifies eval rejects asymmetric property visibility forms that PHP rejects.
#[test]
fn parse_fragment_rejects_invalid_asymmetric_property_visibility() {
    parse_fragment(b"class DynEvalAsymUntyped { public private(set) $id = 1; }")
        .expect_err("asymmetric properties must be typed");
    parse_fragment(b"class DynEvalAsymStatic { public private(set) static int $id = 1; }")
        .expect_err("asymmetric properties cannot be static");
    parse_fragment(b"class DynEvalAsymWeak { private public(set) int $id = 1; }")
        .expect_err("set visibility cannot be weaker than read visibility");
    parse_fragment(b"class DynEvalAsymMethod { public private(set) function run() {} }")
        .expect_err("asymmetric visibility is property-only");
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
                )
                .with_type(Some(EvalParameterType::new(
                    vec![EvalParameterTypeVariant::Int],
                    false
                ))),
                EvalClassProperty::with_visibility_static_and_readonly(
                    "count",
                    EvalVisibility::Public,
                    true,
                    false,
                    Some(EvalExpr::Const(EvalConst::Int(0)))
                )
                .with_type(Some(EvalParameterType::new(
                    vec![EvalParameterTypeVariant::Int],
                    false
                )))
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
            .with_type(Some(EvalParameterType::new(
                vec![EvalParameterTypeVariant::Int],
                false
            )))
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
            .with_type(Some(EvalParameterType::new(
                vec![EvalParameterTypeVariant::Int],
                false
            )))
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
            .with_type(Some(EvalParameterType::new(
                vec![EvalParameterTypeVariant::String],
                false
            )))
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
    parse_fragment(
        b"interface DynEvalIfaceAsymReadOnly { public protected(set) int $id { get; } }",
    )
    .expect_err("readonly virtual property cannot have asymmetric visibility");
    parse_fragment(
        b"interface DynEvalIfaceAsymUntyped { public protected(set) $id { get; set; } }",
    )
    .expect_err("asymmetric interface property must be typed");
}

/// Verifies abstract and final class modifiers lower into dynamic class metadata.
#[test]
fn parse_fragment_accepts_abstract_and_final_class_members() {
    let program = parse_fragment(
        br#"abstract class DynEvalAbstract {
    abstract public function read($value);
    final public $value = 42;
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
            vec![
                EvalClassProperty::with_visibility_static_final_and_readonly(
                    "value",
                    EvalVisibility::Public,
                    false,
                    true,
                    false,
                    Some(EvalExpr::Const(EvalConst::Int(42))),
                )
            ],
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

/// Verifies eval method parameters retain type, default, by-reference, and variadic metadata.
#[test]
fn parse_fragment_accepts_typed_method_parameter_metadata() {
    let program = parse_fragment(
        br#"class DynEvalTypedParams {
    public function run(#[Both("pair")] Left&Right $both, int &$id = 7, ?\App\Name &$name = null, string|null $label = "x", mixed &...$tail) {}
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
        &[
            "both".to_string(),
            "id".to_string(),
            "name".to_string(),
            "label".to_string(),
            "tail".to_string()
        ]
    );
    assert_eq!(
        method.parameter_has_types(),
        &[true, true, true, true, true]
    );
    assert!(method.parameter_types().iter().all(Option::is_some));
    assert_eq!(method.parameter_attributes()[0][0].name(), "Both");
    assert!(method.parameter_attributes()[1].is_empty());
    let both_type = method.parameter_types()[0].as_ref().expect("both type");
    assert_eq!(
        both_type.variants(),
        &[
            EvalParameterTypeVariant::Class("Left".to_string()),
            EvalParameterTypeVariant::Class("Right".to_string())
        ]
    );
    assert!(!both_type.allows_null());
    assert!(both_type.is_intersection());
    let id_type = method.parameter_types()[1].as_ref().expect("id type");
    assert_eq!(id_type.variants(), &[EvalParameterTypeVariant::Int]);
    assert!(!id_type.allows_null());
    assert!(!id_type.is_intersection());
    let name_type = method.parameter_types()[2].as_ref().expect("name type");
    assert_eq!(
        name_type.variants(),
        &[EvalParameterTypeVariant::Class("App\\Name".to_string())]
    );
    assert!(name_type.allows_null());
    assert!(!name_type.is_intersection());
    let label_type = method.parameter_types()[3].as_ref().expect("label type");
    assert_eq!(label_type.variants(), &[EvalParameterTypeVariant::String]);
    assert!(label_type.allows_null());
    assert!(!label_type.is_intersection());
    assert!(matches!(
        method.parameter_defaults(),
        [
            None,
            Some(EvalExpr::Const(EvalConst::Int(7))),
            Some(EvalExpr::Const(EvalConst::Null)),
            Some(EvalExpr::Const(EvalConst::String(label))),
            None
        ] if label == "x"
    ));
    assert_eq!(
        method.parameter_is_by_ref(),
        &[false, true, true, false, true]
    );
    assert_eq!(
        method.parameter_is_variadic(),
        &[false, false, false, false, true]
    );
}

/// Verifies eval method parameter defaults retain supported constant-expression metadata.
#[test]
fn parse_fragment_accepts_method_parameter_constant_defaults() {
    let program = parse_fragment(
        br#"class DynEvalDefaultConstants {
    const LABEL = "box";
    public function read($global = DYN_EVAL_DEFAULT_GLOBAL, $label = self::LABEL, $parent = parent::LABEL, $class = self::class, $items = [self::LABEL => 1 + 2, "fallback" => null ?? "x"], $method = __METHOD__, $dep = new DynEvalDefaultDep(label: "dep")) {}
}"#,
    )
    .expect("fragment should parse");
    let [EvalStmt::ClassDecl(class)] = program.statements() else {
        panic!("expected one class declaration");
    };
    let [method] = class.methods() else {
        panic!("expected one class method");
    };

    assert!(matches!(
        method.parameter_defaults(),
        [
            Some(EvalExpr::ConstFetch(global)),
            Some(EvalExpr::ClassConstantFetch { class_name: self_name, constant: self_constant }),
            Some(EvalExpr::ClassConstantFetch { class_name: parent_name, constant: parent_constant }),
            Some(EvalExpr::ClassNameFetch { class_name }),
            Some(EvalExpr::Array(_)),
            Some(EvalExpr::Magic(EvalMagicConst::Method)),
            Some(EvalExpr::NewObject { class_name: dep_name, args })
        ] if global == "DYN_EVAL_DEFAULT_GLOBAL"
            && self_name == "self"
            && self_constant == "LABEL"
            && parent_name == "parent"
            && parent_constant == "LABEL"
            && class_name == "self"
            && dep_name == "DynEvalDefaultDep"
            && args.len() == 1
            && args[0].name() == Some("label")
    ));
}

/// Verifies eval rejects late-bound `static::` defaults like PHP compile-time constants do.
#[test]
fn parse_fragment_rejects_late_bound_static_method_parameter_defaults() {
    parse_fragment(
        b"class DynEvalStaticDefault { public function read($label = static::LABEL) {} }",
    )
    .expect_err("static class constant defaults are not PHP compile-time constants");
    parse_fragment(
        b"class DynEvalStaticClassDefault { public function read($class = static::class) {} }",
    )
    .expect_err("static class-name defaults are not PHP compile-time constants");
    parse_fragment(
        b"class DynEvalStaticNewDefault { public function read($dep = new static()) {} }",
    )
    .expect_err("static object defaults are not PHP compile-time constants");
    parse_fragment(
        b"class DynEvalSpreadNewDefault { public function read($dep = new DynEvalDep(...[\"x\"])) {} }",
    )
    .expect_err("argument unpacking is not supported in PHP constant expressions");
}

/// Verifies eval rejects invalid variadic method parameter forms.
#[test]
fn parse_fragment_rejects_invalid_variadic_method_parameters() {
    parse_fragment(b"class DynEvalVariadicDefault { public function run(...$tail = null) {} }")
        .expect_err("variadic method parameters cannot have defaults");
    parse_fragment(b"class DynEvalVariadicNotLast { public function run(...$tail, $next) {} }")
        .expect_err("variadic method parameters must be last");
    parse_fragment(b"class DynEvalVariadicRefOrder { public function run(...&$tail) {} }")
        .expect_err("by-reference marker must precede variadic marker");
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
                vec![
                    EvalClassProperty::new("seed", Some(EvalExpr::Const(EvalConst::Int(2))))
                        .with_type(Some(EvalParameterType::new(
                            vec![EvalParameterTypeVariant::Int],
                            false
                        )))
                ],
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

/// Verifies trait declarations can compose other traits with adaptations.
#[test]
fn parse_fragment_accepts_trait_use_inside_trait() {
    let program = parse_fragment(
        br#"trait DynEvalInnerTrait {
    public function read() { return "inner"; }
}
trait DynEvalOuterTrait {
    use DynEvalInnerTrait {
        read as private hiddenRead;
    }
    public function expose() { return $this->hiddenRead(); }
}"#,
    )
    .expect("fragment should parse");

    let [_, EvalStmt::TraitDecl(trait_decl)] = program.statements() else {
        panic!("second statement should be a trait declaration");
    };
    assert_eq!(trait_decl.traits(), &["DynEvalInnerTrait".to_string()]);
    assert_eq!(
        trait_decl.trait_adaptations(),
        &[EvalTraitAdaptation::Alias {
            trait_name: None,
            method: "read".to_string(),
            alias: Some("hiddenRead".to_string()),
            visibility: Some(EvalVisibility::Private),
        }]
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
    for source in [b"return yield 1;" as &[u8]] {
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
