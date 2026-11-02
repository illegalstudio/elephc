//! Purpose:
//! Parser tests for static/global declarations, magic constants, and comments.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases verify source metadata and comment error handling.

use super::support::*;

/// Verifies static local declarations preserve the target name and initializer expression.
#[test]
fn parse_fragment_accepts_static_var_source() {
    let program = parse_fragment(br#"static $n = 1 + 1;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::StaticVar {
            name: "n".to_string(),
            init: EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::Const(EvalConst::Int(1))),
                right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
            },
        }]
    );
}
/// Verifies global declarations preserve source-order variable names.
#[test]
fn parse_fragment_accepts_global_source() {
    let program = parse_fragment(br#"global $left, $right;"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Global {
            vars: vec!["left".to_string(), "right".to_string()],
        }]
    );
}
/// Verifies eval magic constants lower to explicit EvalIR nodes with fragment line metadata.
#[test]
fn parse_fragment_accepts_magic_constants() {
    let program =
        parse_fragment(b"\nreturn __line__ . __FUNCTION__;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::Magic(EvalMagicConst::Line(2))),
            right: Box::new(EvalExpr::Magic(EvalMagicConst::Function)),
        }))]
    );
}
/// Verifies file-dependent eval magic constants lower to EvalIR nodes.
#[test]
fn parse_fragment_accepts_file_magic_constants() {
    let program = parse_fragment(b"return __FILE__ . __dir__;").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::Magic(EvalMagicConst::File)),
            right: Box::new(EvalExpr::Magic(EvalMagicConst::Dir)),
        }))]
    );
}
/// Verifies eval scope magic constants lower with namespace resolved at parse time.
#[test]
fn parse_fragment_accepts_scope_magic_constants() {
    let program = parse_fragment(b"return __CLASS__ . __NAMESPACE__ . __TRAIT__ . __METHOD__;")
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Binary {
            op: EvalBinOp::Concat,
            left: Box::new(EvalExpr::Binary {
                op: EvalBinOp::Concat,
                left: Box::new(EvalExpr::Binary {
                    op: EvalBinOp::Concat,
                    left: Box::new(EvalExpr::Magic(EvalMagicConst::Class)),
                    right: Box::new(EvalExpr::Const(EvalConst::String(String::new()))),
                }),
                right: Box::new(EvalExpr::Magic(EvalMagicConst::Trait)),
            }),
            right: Box::new(EvalExpr::Magic(EvalMagicConst::Method)),
        }))]
    );
}
/// Verifies PHP comments are skipped while preserving fragment line numbers.
#[test]
fn parse_fragment_skips_comments_and_preserves_line_metadata() {
    let program = parse_fragment(b"// leading\n# hash\n/* block\ncomment */ return __LINE__;")
        .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Magic(
            EvalMagicConst::Line(4)
        )))]
    );
}
/// Verifies unterminated block comments fail before partial EvalIR is returned.
#[test]
fn parse_fragment_rejects_unterminated_block_comment() {
    assert_eq!(
        parse_fragment_error(b"/* open").unwrap_err(),
        EvalParseError::UnterminatedComment
    );
}
/// Verifies a doc comment is transparent everywhere the grammar does not read one.
///
/// PHP's `zendlex()` skips `T_DOC_COMMENT` exactly like whitespace and parks the text for the next
/// declaration, so a doc comment inside a parameter list, an argument list or an array literal is
/// trivia. Keeping the token in the stream made `function f(/** @var int */ int $a)` — the shape
/// `symfony/http-kernel/Event/ControllerAttributeEvent.php` writes — a parse error that killed the
/// whole file.
#[test]
fn parse_fragment_skips_a_doc_comment_the_grammar_never_reads() {
    let program = parse_fragment(
        b"function withdoc(/** @var int */ int $a, /** @var int */ int $b): int { return $a + $b; }
return withdoc(/** here */ 1, 2);",
    )
    .expect("fragment should parse");
    assert_eq!(program.statements().len(), 2);
    let program = parse_fragment(b"return [/** k */ 1, 2];").expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Array(vec![
            EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(1))),
            EvalArrayElement::Value(EvalExpr::Const(EvalConst::Int(2))),
        ])))]
    );
}
/// Verifies the one doc comment the grammar does read, a class declaration's, still reaches it.
#[test]
fn parse_fragment_keeps_the_doc_comment_a_class_declaration_reads() {
    let program = parse_fragment(b"/** retained */ final class DocumentedByComment {}")
        .expect("fragment should parse");
    let [EvalStmt::ClassDecl(class)] = program.statements() else {
        panic!(
            "expected one class declaration, got {:?}",
            program.statements()
        );
    };
    assert_eq!(class.doc_comment(), Some("/** retained */"));
}
