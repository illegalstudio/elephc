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

/// Verifies that `yield` is lexed as a keyword when used with a trailing expression.
/// Input: `<?php yield $x;` → expects `Token::Yield` followed by `Token::Variable("x")`.
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

/// Verifies that `yield` without a trailing expression is lexed as a standalone keyword token.
/// Input: `<?php yield;` → expects `Token::Yield` directly followed by `Token::Semicolon`.
#[test]
fn test_yield_alone() {
    let t = tokens("<?php yield;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::Yield, Token::Semicolon, Token::Eof]
    );
}

/// Verifies that `yield from` is lexed as two separate tokens — `Yield` and `Identifier("from")`.
/// The `from` keyword is contextual: it remains an identifier at lex time; the parser
/// handles the `yield from` combination. Input: `<?php yield from $g;`.
#[test]
fn test_yield_from_lexed_as_two_tokens() {
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

/// Verifies that `yield` with a key-value pair (`=>`) is lexed correctly.
/// Input: `<?php yield $k => $v;` → expects `Yield`, variable, `DoubleArrow`, variable.
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
