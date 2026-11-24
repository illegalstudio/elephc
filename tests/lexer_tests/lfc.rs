//! Purpose:
//! Lexer regressions for tagless `.lfc` physical source mode.
//!
//! Called from:
//! - `cargo test --test lexer_tests lfc` through the integration test harness.
//!
//! Key details:
//! - Structural open tags are synthetic and PHP tag text is rejected only at code boundaries.

use elephc::lexer::{tokenize_with_mode, Token};
use elephc::source::SourceMode;

/// Returns token values emitted for one tagless LFC fragment.
fn lfc_tokens(source: &str) -> Vec<Token> {
    tokenize_with_mode(source, SourceMode::Lfc)
        .expect("LFC tokenization should succeed")
        .into_iter()
        .map(|(token, _)| token)
        .collect()
}

/// Verifies empty LFC source still exposes the structural parser boundary.
#[test]
fn lfc_empty_source_synthesizes_open_tag() {
    assert_eq!(lfc_tokens(""), vec![Token::OpenTag, Token::Eof]);
}

/// Verifies code starts at byte one and retains original line and column coordinates.
#[test]
fn lfc_tagless_code_and_bom_keep_source_coordinates() {
    let tokens = tokenize_with_mode("\u{feff}echo 42;", SourceMode::Lfc)
        .expect("BOM-prefixed LFC should tokenize");
    assert_eq!(tokens[0].0, Token::OpenTag);
    assert_eq!(tokens[0].1.span.line, 1);
    assert_eq!(tokens[0].1.span.col, 1);
    assert_eq!(tokens[1].0, Token::Echo);
    assert_eq!(tokens[1].1.span.line, 1);
    assert_eq!(tokens[1].1.span.col, 1);
}

/// Verifies physical PHP opening and closing tags are invalid in LFC code.
#[test]
fn lfc_rejects_php_tags_at_code_boundaries() {
    for source in ["<?php echo 1;", "echo 1; ?>"] {
        let error = tokenize_with_mode(source, SourceMode::Lfc)
            .expect_err("PHP tags must be rejected in LFC");
        assert!(error.message.contains("not valid in .lfc"));
    }
}

/// Verifies tag-shaped bytes in comments and strings remain ordinary data.
#[test]
fn lfc_allows_tag_text_inside_comments_and_strings() {
    let tokens = lfc_tokens("// <?php\n/* ?> */\necho \"<?php ?>\";");
    assert!(tokens.contains(&Token::Echo));
    assert!(tokens.iter().any(|token| {
        matches!(token, Token::StringLiteral(value) if value == "<?php ?>")
    }));
}

/// Preserves compiler comment boundaries and character columns when scanning byte ranges.
#[test]
fn lfc_shared_lexemes_preserve_comment_boundaries_and_spans() {
    let tokens = tokenize_with_mode(
        "// café ?>\r still in the comment\n/*é*/ $名 >>= 0xA_B;",
        SourceMode::Lfc,
    )
    .expect("tag text remains inside the LFC comment");

    let kinds: Vec<_> = tokens.iter().map(|(token, _)| token.clone()).collect();
    assert_eq!(kinds, [
        Token::OpenTag,
        Token::Variable("名".into()),
        Token::GreaterGreaterAssign,
        Token::IntLiteral(171),
        Token::Semicolon,
        Token::Eof,
    ]);
    let span = tokens[1].1.span;
    assert_eq!((span.line, span.col, span.end_column()), (2, 7, 9));
    assert_eq!(tokens[2].1.span.col, 10);
}
