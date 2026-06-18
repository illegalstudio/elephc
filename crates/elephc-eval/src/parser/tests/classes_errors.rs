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
