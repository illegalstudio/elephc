//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of PHP source structure, including open tag, line comment, and block comment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

/// Verifies `<?php` produces `OpenTag` and EOF, the bare minimum valid PHP script.
#[test]
fn test_open_tag() {
    let t = tokens("<?php");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

/// Verifies a leading UTF-8 BOM (U+FEFF) before `<?php` is stripped, so files saved by
/// editors that emit BOM-prefixed UTF-8 still tokenize starting at `OpenTag`.
#[test]
fn test_utf8_bom_before_open_tag_is_stripped() {
    let t = tokens("\u{feff}<?php echo \"hi\";");
    assert_eq!(t[0], Token::OpenTag);
    assert_eq!(t[1], Token::Echo);
}

/// Verifies `// ...` line comments are consumed and do not appear in the token stream.
#[test]
fn test_line_comment() {
    let t = tokens("<?php // this is a comment\necho \"hi\";");
    assert_eq!(t[1], Token::Echo);
}

/// Verifies `/* ... */` block comments are consumed and do not appear in the token stream.
#[test]
fn test_block_comment() {
    let t = tokens("<?php /* block */ echo \"hi\";");
    assert_eq!(t[1], Token::Echo);
}

/// Verifies consecutive comments (block and line) are all skipped correctly.
#[test]
fn test_consecutive_comments() {
    let t = tokens("<?php /* a *//* b */// c\necho \"ok\";");
    assert_eq!(t[1], Token::Echo);
}

// --- Complex tokens ---

/// Verifies a source with no open tag is inline HTML (PHP prints it), not a lex error.
#[test]
fn test_missing_open_tag_is_inline_html() {
    let t = tokens("echo \"hi\";");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::StringLiteral("echo \"hi\";".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// Verifies an unterminated double-quoted string produces a lex error.
#[test]
fn test_unterminated_string() {
    assert!(tokenize("<?php \"no closing").is_err());
}

// --- Spans ---

/// Verifies line tracking: `echo` on line 2 reports line=2, col=1.
#[test]
fn test_span_tracking() {
    let spanned = tokenize("<?php\necho \"hi\";").unwrap();
    let echo_span = spanned[1].1.span;
    assert_eq!(echo_span.line, 2);
    assert_eq!(echo_span.col, 1);
}

/// Verifies multiline sources report the correct line number for the last token.
#[test]
fn test_span_multiline() {
    let spanned = tokenize("<?php\n\n\n$x").unwrap();
    let var_span = spanned[1].1.span;
    assert_eq!(var_span.line, 4);
}

// --- Strict comparison ---

/// Verifies trailing space after `<?php` still produces only `OpenTag` + `Eof`.
#[test]
fn test_empty_after_open_tag() {
    let t = tokens("<?php ");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

/// Verifies `<?php` with no trailing whitespace produces `OpenTag` + `Eof`.
#[test]
fn test_open_tag_no_trailing_space() {
    let t = tokens("<?php");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

/// Verifies `<?php\n` (newline only after open tag) produces `OpenTag` + `Eof`.
#[test]
fn test_open_tag_newline_only() {
    let t = tokens("<?php\n");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

/// Verifies a line comment after open tag with no trailing code produces `OpenTag` + `Eof`.
#[test]
fn test_open_tag_with_comment_no_code() {
    let t = tokens("<?php // nothing here\n");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

/// Verifies a block comment after open tag with no trailing code produces `OpenTag` + `Eof`.
#[test]
fn test_open_tag_with_block_comment_no_code() {
    let t = tokens("<?php /* empty */");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

// --- Inline HTML outside PHP tags ---

/// Verifies leading HTML becomes `echo "text";` after the structural open tag.
#[test]
fn test_leading_inline_html_is_echoed() {
    let t = tokens("Hello <?php echo 1;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::StringLiteral("Hello ".into()),
            Token::Semicolon,
            Token::Echo,
            Token::IntLiteral(1),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// Verifies `?>` is a `;`, swallows one newline, and the HTML after it is echoed.
#[test]
fn test_close_tag_swallows_one_newline_and_echoes_html() {
    let t = tokens("<?php echo 1 ?>\n\nx<?php echo 2;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::IntLiteral(1),
            Token::Semicolon,
            Token::Echo,
            Token::StringLiteral("\nx".into()),
            Token::Semicolon,
            Token::Echo,
            Token::IntLiteral(2),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// Verifies `<?=` is `echo`, and a file ending in `?>` produces no trailing output.
#[test]
fn test_short_echo_tag_and_trailing_close_tag() {
    let t = tokens("<?= 5 ?>\n");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::IntLiteral(5),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// Verifies `<?phpx` and `<?xml` are plain text, not open tags, and the short `<?` is text.
#[test]
fn test_non_tags_stay_html() {
    let t = tokens("<?xml a?> <?phpx <? q ?>");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::StringLiteral("<?xml a?> <?phpx <? q ?>".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// Verifies a `//` comment ends at `?>`, which then closes the PHP block.
#[test]
fn test_line_comment_ends_at_close_tag() {
    let t = tokens("<?php // c ?>HTML");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Semicolon,
            Token::Echo,
            Token::StringLiteral("HTML".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// Verifies line numbers after inline HTML stay exact.
#[test]
fn test_span_lines_after_inline_html() {
    let spanned = tokenize("a\nb\n<?php\n$x").unwrap();
    let var = spanned.iter().find(|(t, _)| matches!(t, Token::Variable(_))).unwrap();
    assert_eq!(var.1.span.line, 4);
}
