//! Purpose:
//! Parser tests for abstract/final methods, typed/default parameters, traits,
//! object construction, and terminal diagnostics.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Parameter metadata and invalid variadic or late-bound defaults are retained.

use super::super::support::*;

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
