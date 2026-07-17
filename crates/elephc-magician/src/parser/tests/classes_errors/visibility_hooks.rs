//! Purpose:
//! Parser tests for member visibility, readonly/asymmetric properties, property
//! hooks, and related invalid forms.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Concrete, abstract, interface, and trait hook contracts are distinguished.

use super::super::support::*;

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
        &get => 7;
        set => $value + 1;
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
            .with_hooks(true, true)
            .with_virtual(false)],
            vec![
                EvalClassMethod::new(
                    "__propget_value",
                    Vec::new(),
                    vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::Int(7))))]
                )
                .with_source_location(EvalSourceLocation::new(3, 3)),
                EvalClassMethod::new(
                    "__propset_value",
                    vec!["value".to_string()],
                    vec![EvalStmt::PropertySet {
                        object: EvalExpr::LoadVar("this".to_string()),
                        property: "value".to_string(),
                        value: EvalExpr::Binary {
                            op: EvalBinOp::Add,
                            left: Box::new(EvalExpr::LoadVar("value".to_string())),
                            right: Box::new(EvalExpr::Const(EvalConst::Int(1)))
                        }
                    }]
                )
                .with_parameter_types(vec![Some(EvalParameterType::new(
                    vec![EvalParameterTypeVariant::Int],
                    false
                ))])
                .with_source_location(EvalSourceLocation::new(4, 4))
            ]
        )
        .with_source_location(EvalSourceLocation::new(1, 6)))]
    );
}

/// Verifies typed set-hook parameters are retained separately from the property type.
#[test]
fn parse_fragment_retains_property_set_hook_parameter_type() {
    let program = parse_fragment(
        br#"class DynEvalTypedSetHooked {
    public string $value {
        set(int|string $raw) => $raw;
    }
}"#,
    )
    .expect("fragment should parse");
    let EvalStmt::ClassDecl(class) = &program.statements()[0] else {
        panic!("expected class declaration");
    };
    let property = &class.properties()[0];
    assert_eq!(
        property.property_type(),
        Some(&EvalParameterType::new(
            vec![EvalParameterTypeVariant::String],
            false
        ))
    );
    assert_eq!(
        property.set_hook_type(),
        Some(&EvalParameterType::new(
            vec![
                EvalParameterTypeVariant::Int,
                EvalParameterTypeVariant::String
            ],
            false
        ))
    );
    assert_eq!(
        class.methods()[0].parameter_types(),
        &[Some(EvalParameterType::new(
            vec![
                EvalParameterTypeVariant::Int,
                EvalParameterTypeVariant::String
            ],
            false
        ))]
    );
}

/// Verifies eval rejects explicit untyped set-hook parameters for typed properties.
#[test]
fn parse_fragment_rejects_untyped_explicit_property_set_hook_parameter_for_typed_property() {
    assert_eq!(
        parse_fragment(
            br#"class DynEvalUntypedSetParamHooked {
    public string $value {
        set($raw) => $raw;
    }
}"#
        ),
        Err(EvalParseError::UnsupportedConstruct)
    );

    parse_fragment(
        br#"class DynEvalUntypedSetParamUntypedProperty {
    public $value {
        set($raw) => $raw;
    }
}"#,
    )
    .expect("untyped properties may use an untyped explicit set-hook parameter");
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
    parse_fragment(b"class DynEvalHookByRefSet { public int $id { &set => 1; } }")
        .expect_err("set property hooks cannot return by reference");
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
    parse_fragment(b"interface DynEvalIfaceHookByRefSet { public int $id { &set; } }")
        .expect_err("set interface property hooks cannot return by reference");
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
