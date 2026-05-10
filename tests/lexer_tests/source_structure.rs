//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of PHP source structure, including open tag, line comment, and block comment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

#[test]
fn test_open_tag() {
    let t = tokens("<?php");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

#[test]
fn test_line_comment() {
    let t = tokens("<?php // this is a comment\necho \"hi\";");
    assert_eq!(t[1], Token::Echo);
}

#[test]
fn test_block_comment() {
    let t = tokens("<?php /* block */ echo \"hi\";");
    assert_eq!(t[1], Token::Echo);
}

#[test]
fn test_consecutive_comments() {
    let t = tokens("<?php /* a *//* b */// c\necho \"ok\";");
    assert_eq!(t[1], Token::Echo);
}

// --- Complex tokens ---

#[test]
fn test_missing_open_tag() {
    assert!(tokenize("echo \"hi\";").is_err());
}

#[test]
fn test_unterminated_string() {
    assert!(tokenize("<?php \"no closing").is_err());
}

// --- Spans ---

#[test]
fn test_span_tracking() {
    let spanned = tokenize("<?php\necho \"hi\";").unwrap();
    let echo_span = spanned[1].1;
    assert_eq!(echo_span.line, 2);
    assert_eq!(echo_span.col, 1);
}

#[test]
fn test_span_multiline() {
    let spanned = tokenize("<?php\n\n\n$x").unwrap();
    let var_span = spanned[1].1;
    assert_eq!(var_span.line, 4);
}

// --- Strict comparison ---

#[test]
fn test_empty_after_open_tag() {
    let t = tokens("<?php ");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

#[test]
fn test_open_tag_no_trailing_space() {
    let t = tokens("<?php");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

#[test]
fn test_open_tag_newline_only() {
    let t = tokens("<?php\n");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

#[test]
fn test_open_tag_with_comment_no_code() {
    let t = tokens("<?php // nothing here\n");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

#[test]
fn test_open_tag_with_block_comment_no_code() {
    let t = tokens("<?php /* empty */");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}
