//! Purpose:
//! Parser regressions for tagless `.lfc` source and durable source provenance.
//!
//! Called from:
//! - `cargo test --test integration parser_tests::lfc` through the integration test harness.
//!
//! Key details:
//! - LFC uses the shared PHP grammar after the lexer synthesizes `OpenTag`.

use elephc::lexer::tokenize_with_mode;
use elephc::parser::ast::{ExprKind, StmtKind};
use elephc::parser::parse_with_mode;
use elephc::source::SourceMode;

/// Parses one tagless LFC fragment through the mode-aware frontend boundary.
fn parse_lfc(source: &str) -> elephc::parser::ast::Program {
    let tokens = tokenize_with_mode(source, SourceMode::Lfc).expect("LFC lexing should succeed");
    parse_with_mode(&tokens, SourceMode::Lfc).expect("LFC parsing should succeed")
}

/// Verifies tagless statements retain their physical source profile.
#[test]
fn lfc_parses_tagless_code_with_lfc_provenance() {
    let program = parse_lfc("echo strlen(\"ok\");");
    assert_eq!(program[0].source_mode, SourceMode::Lfc);
    let StmtKind::Echo(expr) = &program[0].kind else {
        panic!("expected echo statement");
    };
    assert!(matches!(
        expr.kind,
        ExprKind::FunctionCall { .. }
    ));
}

/// Verifies plain prose is parsed as invalid code instead of becoming implicit output.
#[test]
fn lfc_plain_text_is_not_implicit_output() {
    let tokens =
        tokenize_with_mode("this is not source code", SourceMode::Lfc).expect("lexing succeeds");
    assert!(parse_with_mode(&tokens, SourceMode::Lfc).is_err());
}
