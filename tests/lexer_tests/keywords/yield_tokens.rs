//! Purpose:
//! Lexer tests for `yield` and contextual `yield from` tokenization.
//!
//! Called from:
//!  - `cargo test` through Rust's test harness via `tests/lexer_tests.rs`.
//!
//! Key details:
//!  - `from` remains an identifier at lex time; the parser interprets it
//!    contextually after `yield`.

use super::*;

#[test]
fn test_yield_keyword() {
    let t = tokens("<?php yield $x;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Yield,
            Token::Variable("x".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_yield_alone() {
    let t = tokens("<?php yield;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::Yield, Token::Semicolon, Token::Eof]
    );
}

#[test]
fn test_yield_from_lexed_as_two_tokens() {
    // `from` is a contextual keyword — it remains an Identifier at lex time.
    let t = tokens("<?php yield from $g;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Yield,
            Token::Identifier("from".into()),
            Token::Variable("g".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_yield_key_value() {
    let t = tokens("<?php yield $k => $v;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Yield,
            Token::Variable("k".into()),
            Token::DoubleArrow,
            Token::Variable("v".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}
