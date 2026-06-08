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

/// Verifies missing `<?php` open tag produces a lex error.
#[test]
fn test_missing_open_tag() {
    assert!(tokenize("echo \"hi\";").is_err());
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
    let echo_span = spanned[1].1;
    assert_eq!(echo_span.line, 2);
    assert_eq!(echo_span.col, 1);
}

/// Verifies multiline sources report the correct line number for the last token.
#[test]
fn test_span_multiline() {
    let spanned = tokenize("<?php\n\n\n$x").unwrap();
    let var_span = spanned[1].1;
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
