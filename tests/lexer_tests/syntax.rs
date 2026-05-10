//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of syntax, including variable, braces, and parens.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

#[test]
fn test_variable() {
    let t = tokens("<?php $foo");
    assert_eq!(t[1], Token::Variable("foo".into()));
}

// --- Operators ---

#[test]
fn test_braces() {
    let t = tokens("<?php { }");
    assert_eq!(t[1..3], [Token::LBrace, Token::RBrace]);
}

#[test]
fn test_parens() {
    let t = tokens("<?php ( )");
    assert_eq!(t[1..3], [Token::LParen, Token::RParen]);
}

#[test]
fn test_comma() {
    let t = tokens("<?php ,");
    assert_eq!(t[1], Token::Comma);
}

// --- Keywords ---

#[test]
fn test_identifier() {
    assert_eq!(
        tokens("<?php foo")[1],
        Token::Identifier("foo".into())
    );
}

// --- Comments ---

#[test]
fn test_equals_vs_assign() {
    // = followed by = should be ==, not two Assigns
    let t = tokens("<?php == =");
    assert_eq!(t[1], Token::EqualEqual);
    assert_eq!(t[2], Token::Assign);
}

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
