//! Purpose:
//! Parser tests for whole PHP source files: inline HTML, `<?php` blocks, and closing tags.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - A source file is one token stream, so a brace one block opens and a later block closes is
//!   ordinary nesting rather than an unterminated body.
//! - A FILE parse also emits an `EvalStmt::SourceLine` marker whenever the line moves, which is
//!   what lets an included file's diagnostics, `debug_backtrace()` frames and `eval()`'d-code
//!   spellings name the statement running rather than the line the include began on. Fragments
//!   emit none, which is why every statement-shape expectation written against `parse_fragment`
//!   is unaffected -- these four files are the whole blast radius, and they are the tests that
//!   document the markers.

use super::super::parse_source_file;
use super::support::*;

/// Verifies a brace opened in one PHP block and closed in a later one parses as one statement.
///
/// Every template under `symfony/error-handler/Resources/views` is written this way, and parsing
/// each `<?php … ?>` block on its own could only ever report an unexpected end of file.
#[test]
fn parse_source_file_spans_a_brace_across_two_php_blocks() {
    let program = parse_source_file(b"<?php if (true) { ?>X<?php } ?>\n")
        .expect("source file should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::SourceLine(1),
            EvalStmt::If {
                condition: EvalExpr::Const(EvalConst::Bool(true)),
                then_branch: vec![EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
                    "X".to_string()
                )))],
                else_branch: Vec::new(),
            }
        ]
    );
}

/// Verifies the alternative-syntax form templates use spans blocks the same way.
#[test]
fn parse_source_file_spans_an_alternative_body_across_two_php_blocks() {
    let program = parse_source_file(b"<?php if ($a): ?>yes<?php endif ?>\n")
        .expect("source file should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::SourceLine(1),
            EvalStmt::If {
                condition: EvalExpr::LoadVar("a".to_string()),
                then_branch: vec![EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
                    "yes".to_string()
                )))],
                else_branch: Vec::new(),
            }
        ]
    );
}

/// Verifies leading and trailing inline HTML become echoes and `<?=` echoes its expression.
///
/// PHP swallows exactly one newline after a closing tag, which is why the text between the two
/// tags below is `b` and not `\nb`.
#[test]
fn parse_source_file_echoes_inline_html_and_short_echo_tags() {
    let program = parse_source_file(b"a<?= $x ?>\nb<?php $y = 1; ?>\nc")
        .expect("source file should parse");
    assert_eq!(
        program.statements(),
        &[
            // The leading HTML and the `<?=` that follows it are both on line 1, so they share
            // one marker: a marker is emitted when the line MOVES, not per statement.
            EvalStmt::SourceLine(1),
            EvalStmt::Echo(EvalExpr::Const(EvalConst::String("a".to_string()))),
            EvalStmt::Echo(EvalExpr::LoadVar("x".to_string())),
            EvalStmt::SourceLine(2),
            EvalStmt::Echo(EvalExpr::Const(EvalConst::String("b".to_string()))),
            EvalStmt::StoreVar {
                name: "y".to_string(),
                value: EvalExpr::Const(EvalConst::Int(1)),
            },
            EvalStmt::SourceLine(3),
            EvalStmt::Echo(EvalExpr::Const(EvalConst::String("c".to_string()))),
        ]
    );
}

/// Verifies a file with no PHP tag at all is one inline-HTML echo.
#[test]
fn parse_source_file_treats_a_file_without_tags_as_inline_html() {
    let program = parse_source_file(b"<html>plain</html>").expect("source file should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::SourceLine(1),
            EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
                "<html>plain</html>".to_string()
            )))
        ]
    );
}

/// Verifies a diagnostic names the file line, with no per-block offset left to add.
#[test]
fn parse_source_file_reports_the_file_line_of_a_refusal() {
    let diagnostic = parse_source_file(b"one\n<?php\n$a = 1;\n?>\ntwo\n<?php\n$b = ;\n")
        .expect_err("the second block is not valid PHP");
    assert_eq!(diagnostic.line(), 7);
    assert_eq!(diagnostic.token(), "token \";\"");
}

/// Verifies the `;` a closing tag leaves in front of the first `case` is legal in both forms.
#[test]
fn parse_source_file_accepts_a_closing_tag_before_the_first_case() {
    for source in [
        &b"<?php switch ($k): ?>\n<?php case 1: ?>A<?php break; ?>\n<?php endswitch; ?>"[..],
        &b"<?php switch ($k) { ?>\n<?php case 1: ?>A<?php break; ?>\n<?php } ?>"[..],
    ] {
        let program = parse_source_file(source).expect("a closing tag before a case should parse");
        assert!(program.statements().iter().any(
            |statement| matches!(statement, EvalStmt::Switch { cases, .. } if cases.len() == 1)
        ));
    }
}

/// Verifies `<?php` glued to another byte is inline HTML, not an open tag.
#[test]
fn parse_source_file_treats_a_glued_php_open_tag_as_text() {
    let program =
        parse_source_file(b"see <?phpunit bootstrap ?>\n").expect("inline text should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::SourceLine(1),
            EvalStmt::Echo(EvalExpr::Const(EvalConst::String(
                "see <?phpunit bootstrap ?>\n".to_string()
            )))
        ]
    );
}

/// Verifies a statement after an empty one on the same line still gets its line marker.
#[test]
fn parse_source_file_marks_the_line_after_an_empty_statement() {
    let program = parse_source_file(b"<?php\nif (1) {\n$a = 1;\n} ?><p><?= $b ?>")
        .expect("source file should parse");
    let lines: Vec<i64> = program
        .statements()
        .iter()
        .filter_map(|statement| match statement {
            EvalStmt::SourceLine(line) => Some(*line),
            _ => None,
        })
        .collect();
    assert_eq!(lines, vec![2, 4]);
}
