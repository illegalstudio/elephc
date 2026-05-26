//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of spread syntax and calls, including ellipsis token, ellipsis in function params, and ellipsis in function call.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

// Verifies `...` standalone tokenizes as `Token::Ellipsis` — the three-dot spread
// operator distinct from the binary concat `Token::Dot`.
#[test]
fn test_ellipsis_token() {
    let t = tokens("<?php ...");
    assert_eq!(t, vec![Token::OpenTag, Token::Ellipsis, Token::Eof]);
}

// Verifies `...` before a variadic parameter in a function declaration produces
// `Token::Ellipsis` before `Token::Variable`. Regression guard for parameter
// position ordering.
#[test]
fn test_ellipsis_in_function_params() {
    let t = tokens("<?php function foo(...$args) {}");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Function,
            Token::Identifier("foo".into()),
            Token::LParen,
            Token::Ellipsis,
            Token::Variable("args".into()),
            Token::RParen,
            Token::LBrace,
            Token::RBrace,
            Token::Eof,
        ]
    );
}

// Verifies `...` before an argument in a function call produces `Token::Ellipsis`
// before `Token::Variable`. Tests the call-argument spread position (not params).
#[test]
fn test_ellipsis_in_function_call() {
    let t = tokens("<?php foo(...$arr);");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Identifier("foo".into()),
            Token::LParen,
            Token::Ellipsis,
            Token::Variable("arr".into()),
            Token::RParen,
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

// Verifies named arguments `name: "Alice"` tokenize as identifier, colon, then value.
// Each part of a named argument is a separate token — the name is NOT a string literal.
#[test]
fn test_named_arguments_tokens() {
    let t = tokens("<?php foo(name: \"Alice\", age: 30);");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Identifier("foo".into()),
            Token::LParen,
            Token::Identifier("name".into()),
            Token::Colon,
            Token::StringLiteral("Alice".into()),
            Token::Comma,
            Token::Identifier("age".into()),
            Token::Colon,
            Token::IntLiteral(30),
            Token::RParen,
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

// Verifies `.` (concat) and `...` (ellipsis) do not get confused. Single dot is
// `Token::Dot`; three dots in a row are `Token::Ellipsis`. Regression guard for
// lexer precedence when both operators appear in the same expression.
#[test]
fn test_dot_vs_ellipsis() {
    // Single dot is concat, three dots is ellipsis
    let t = tokens("<?php $a . $b ... $c");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Variable("a".into()),
            Token::Dot,
            Token::Variable("b".into()),
            Token::Ellipsis,
            Token::Variable("c".into()),
            Token::Eof,
        ]
    );
}
