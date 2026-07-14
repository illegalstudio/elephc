//! Purpose:
//! Parser tests for basic class declarations, types, reserved names, references,
//! and class-like constants.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover dynamic class metadata and malformed fragment errors.

use super::super::support::*;

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

/// Verifies eval rejects PHP's reserved `class` class-constant name.
#[test]
fn parse_fragment_rejects_reserved_class_constant_name() {
    for source in [
        b"class DynEvalBadConstName { const class = 1; }" as &[u8],
        b"interface DynEvalBadIfaceConstName { const class = 1; }",
        b"trait DynEvalBadTraitConstName { const class = 1; }",
        b"enum DynEvalBadEnumConstName { const class = 1; }",
    ] {
        assert_eq!(
            parse_fragment(source),
            Err(EvalParseError::UnsupportedConstruct)
        );
    }
}

/// Verifies eval rejects PHP-reserved class-like declaration names.
#[test]
fn parse_fragment_rejects_reserved_class_like_declaration_names() {
    for source in [
        b"class match {}" as &[u8],
        b"class string {}",
        b"class CLASS {}",
        b"interface interface {}",
        b"trait readonly {}",
        b"enum bool { case Ready; }",
    ] {
        assert_eq!(
            parse_fragment(source),
            Err(EvalParseError::UnsupportedConstruct)
        );
    }
}

/// Verifies eval accepts PHP semi-reserved class-like declaration names.
#[test]
fn parse_fragment_accepts_semi_reserved_class_like_declaration_names() {
    let program = parse_fragment(
        br#"class enum {}
interface from {}
trait resource {}
enum integer { case Ready; }"#,
    )
    .expect("fragment should parse");
    let statements = program.statements();
    let EvalStmt::ClassDecl(class) = &statements[0] else {
        panic!("expected class declaration");
    };
    assert_eq!(class.name(), "enum");

    let EvalStmt::InterfaceDecl(interface) = &statements[1] else {
        panic!("expected interface declaration");
    };
    assert_eq!(interface.name(), "from");

    let EvalStmt::TraitDecl(trait_decl) = &statements[2] else {
        panic!("expected trait declaration");
    };
    assert_eq!(trait_decl.name(), "resource");

    let EvalStmt::EnumDecl(enum_decl) = &statements[3] else {
        panic!("expected enum declaration");
    };
    assert_eq!(enum_decl.name(), "integer");
}

/// Verifies eval rejects PHP-reserved bare class-like reference names.
#[test]
fn parse_fragment_rejects_reserved_unqualified_class_like_reference_names() {
    for source in [
        b"class DynEvalBadExtends extends match {}" as &[u8],
        b"class DynEvalBadImplements implements match {}",
        b"interface DynEvalBadIfaceExtends extends match {}",
        b"class DynEvalBadTraitUse { use match; }",
        b"class DynEvalBadTraitAdapt { use SomeTrait { match::run insteadof SomeTrait; } }",
        b"$box = new match();",
        b"$ok = $box instanceof match;",
        b"try {} catch (match $e) {}",
    ] {
        assert_eq!(
            parse_fragment(source),
            Err(EvalParseError::UnsupportedConstruct)
        );
    }
}

/// Verifies eval accepts semi-reserved or qualified class-like reference names PHP parses.
#[test]
fn parse_fragment_accepts_semi_reserved_and_qualified_class_like_reference_names() {
    parse_fragment(
        br#"class enum {}
class DynEvalExtendsSemiReserved extends enum {}
class DynEvalExtendsQualifiedReserved extends \match {}
class DynEvalNewSelf {
    public function make() { return new self(); }
}
$ok = $box instanceof \match;"#,
    )
    .expect("fragment should parse");
}

/// Verifies comma-separated class-like constants lower to individual eval constants.
#[test]
fn parse_fragment_accepts_comma_separated_class_like_constants() {
    let program = parse_fragment(
        br#"class DynEvalMultiConstClass {
    public const A = 1, B = 2;
}
interface DynEvalMultiConstIface {
    final public const C = 3, D = 4;
}
trait DynEvalMultiConstTrait {
    protected const E = 5, F = 6;
}
enum DynEvalMultiConstEnum {
    public const G = 7, H = 8;
    case Ready;
}"#,
    )
    .expect("fragment should parse");
    let statements = program.statements();
    let EvalStmt::ClassDecl(class) = &statements[0] else {
        panic!("expected class declaration");
    };
    assert_eq!(
        class.constants(),
        &[
            EvalClassConstant::new("A", EvalExpr::Const(EvalConst::Int(1))),
            EvalClassConstant::new("B", EvalExpr::Const(EvalConst::Int(2))),
        ],
    );
    let EvalStmt::InterfaceDecl(interface) = &statements[1] else {
        panic!("expected interface declaration");
    };
    assert_eq!(
        interface.constants(),
        &[
            EvalClassConstant::with_visibility_and_final(
                "C",
                EvalVisibility::Public,
                true,
                EvalExpr::Const(EvalConst::Int(3)),
            ),
            EvalClassConstant::with_visibility_and_final(
                "D",
                EvalVisibility::Public,
                true,
                EvalExpr::Const(EvalConst::Int(4)),
            ),
        ],
    );
    let EvalStmt::TraitDecl(trait_decl) = &statements[2] else {
        panic!("expected trait declaration");
    };
    assert_eq!(
        trait_decl.constants(),
        &[
            EvalClassConstant::with_visibility(
                "E",
                EvalVisibility::Protected,
                EvalExpr::Const(EvalConst::Int(5)),
            ),
            EvalClassConstant::with_visibility(
                "F",
                EvalVisibility::Protected,
                EvalExpr::Const(EvalConst::Int(6)),
            ),
        ],
    );
    let EvalStmt::EnumDecl(enum_decl) = &statements[3] else {
        panic!("expected enum declaration");
    };
    assert_eq!(
        enum_decl.constants(),
        &[
            EvalClassConstant::new("G", EvalExpr::Const(EvalConst::Int(7))),
            EvalClassConstant::new("H", EvalExpr::Const(EvalConst::Int(8))),
        ],
    );
}
