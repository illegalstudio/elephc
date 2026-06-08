//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of syntax, including variable, braces, and parens.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

/// Verifies `$foo` tokenizes as `Variable("foo")`.
#[test]
fn test_variable() {
    let t = tokens("<?php $foo");
    assert_eq!(t[1], Token::Variable("foo".into()));
}

/// Verifies a variable name with non-ASCII letters (PHP allows bytes 0x80-0xFF in
/// identifiers) lexes as one `Variable` token instead of truncating at the first
/// non-ASCII byte.
#[test]
fn test_unicode_variable() {
    let t = tokens("<?php $café");
    assert_eq!(t[1], Token::Variable("café".into()));
}

/// Verifies an identifier made of non-ASCII letters lexes as a single `Identifier`
/// token instead of erroring as an unexpected character.
#[test]
fn test_unicode_identifier() {
    let t = tokens("<?php 价格");
    assert_eq!(t[1], Token::Identifier("价格".into()));
}

// --- Operators ---

/// Verifies `{` and `}` tokenize as `LBrace` / `RBrace`.
#[test]
fn test_braces() {
    let t = tokens("<?php { }");
    assert_eq!(t[1..3], [Token::LBrace, Token::RBrace]);
}

/// Verifies `(` and `)` tokenize as `LParen` / `RParen`.
#[test]
fn test_parens() {
    let t = tokens("<?php ( )");
    assert_eq!(t[1..3], [Token::LParen, Token::RParen]);
}

/// Verifies `,` tokenizes as `Comma`.
#[test]
fn test_comma() {
    let t = tokens("<?php ,");
    assert_eq!(t[1], Token::Comma);
}

// --- Keywords ---

/// Verifies `foo` (bare identifier) tokenizes as `Identifier("foo")`.
#[test]
fn test_identifier() {
    assert_eq!(
        tokens("<?php foo")[1],
        Token::Identifier("foo".into())
    );
}

// --- Comments ---

/// Verifies `==` is not two `Assign` tokens (no token fusion regression).
#[test]
fn test_equals_vs_assign() {
    // = followed by = should be ==, not two Assigns
    let t = tokens("<?php == =");
    assert_eq!(t[1], Token::EqualEqual);
    assert_eq!(t[2], Token::Assign);
}

/// Verifies `global $a, $b;` produces the correct token sequence with comma.
#[test]
fn test_global_multiple() {
    let t = tokens("<?php global $a, $b;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Global,
            Token::Variable("a".into()),
            Token::Comma,
            Token::Variable("b".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

// --- Static keyword ---

/// Verifies `&$x` (ref parameter) produces `Ampersand` + `Variable` in params.
#[test]
fn test_ref_param_in_function() {
    let t = tokens("<?php function foo(&$x) {}");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Function,
            Token::Identifier("foo".into()),
            Token::LParen,
            Token::Ampersand,
            Token::Variable("x".into()),
            Token::RParen,
            Token::LBrace,
            Token::RBrace,
            Token::Eof,
        ]
    );
}

// --- Hex integer literals ---
