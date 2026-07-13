//! Purpose:
//! Parser tests for interfaces, property contracts, constructor promotion, and
//! public class members.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Interface and property forms retain their EvalIR metadata and diagnostics.

use super::super::support::*;

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
    public int $id { &get; }
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

/// Verifies comma-separated simple properties lower to individual eval properties.
#[test]
fn parse_fragment_accepts_comma_separated_class_properties() {
    let program = parse_fragment(
        br#"class DynEvalMultiProperties {
    public private(set) int $id = 1, $nextId;
    public static string $first = "a", $second = "b";
    var $legacy, $legacyDefault = 3;
}
trait DynEvalMultiPropertyTrait {
    protected int $left = 4, $right = 5;
}"#,
    )
    .expect("fragment should parse");
    let EvalStmt::ClassDecl(class) = &program.statements()[0] else {
        panic!("expected class declaration");
    };
    let properties = class.properties();
    assert_eq!(properties.len(), 6);
    assert_eq!(properties[0].name(), "id");
    assert_eq!(properties[0].set_visibility(), Some(EvalVisibility::Private));
    assert_eq!(
        properties[0].property_type(),
        Some(&EvalParameterType::new(
            vec![EvalParameterTypeVariant::Int],
            false,
        )),
    );
    assert_eq!(properties[0].default(), Some(&EvalExpr::Const(EvalConst::Int(1))));
    assert_eq!(properties[1].name(), "nextId");
    assert_eq!(properties[1].set_visibility(), Some(EvalVisibility::Private));
    assert_eq!(properties[1].default(), None);
    assert_eq!(properties[2].name(), "first");
    assert!(properties[2].is_static());
    assert_eq!(
        properties[2].property_type(),
        Some(&EvalParameterType::new(
            vec![EvalParameterTypeVariant::String],
            false,
        )),
    );
    assert_eq!(properties[3].name(), "second");
    assert!(properties[3].is_static());
    assert_eq!(properties[4].name(), "legacy");
    assert_eq!(properties[4].property_type(), None);
    assert_eq!(properties[5].name(), "legacyDefault");
    assert_eq!(properties[5].default(), Some(&EvalExpr::Const(EvalConst::Int(3))));

    let EvalStmt::TraitDecl(trait_decl) = &program.statements()[1] else {
        panic!("expected trait declaration");
    };
    assert_eq!(trait_decl.properties().len(), 2);
    assert_eq!(trait_decl.properties()[0].name(), "left");
    assert_eq!(trait_decl.properties()[0].visibility(), EvalVisibility::Protected);
    assert_eq!(trait_decl.properties()[1].name(), "right");
    assert_eq!(trait_decl.properties()[1].visibility(), EvalVisibility::Protected);
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
