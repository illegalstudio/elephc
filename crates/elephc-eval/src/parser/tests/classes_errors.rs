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
    parse_fragment(b"class DynEvalHookAbstract { public int $id { get; } }")
        .expect_err("abstract property hooks are not supported in eval classes");
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
