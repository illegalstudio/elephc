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

/// Verifies `(object)` tokenizes as the ordinary three-token cast window the parser's
/// `peek_cast` matches — `LParen`, a bare `Identifier`, `RParen` — and NOT as a keyword.
///
/// `object` is a type name, not a reserved word: PHP lets it be a function name, a class name
/// and a namespace segment, so giving it a token of its own would break those. The cast is a
/// PARSER shape over ordinary tokens, which is what this pins.
#[test]
fn test_object_cast_lexes_as_a_plain_identifier_window() {
    let t = tokens("<?php (object)$v;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::LParen,
            Token::Identifier("object".into()),
            Token::RParen,
            Token::Variable("v".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// The spelling is case-insensitive like every PHP cast, and the lexer preserves the source
/// casing in the `Identifier` — the parser is what compares case-insensitively.
#[test]
fn test_object_cast_keeps_its_source_casing_in_the_identifier() {
    let t = tokens("<?php (OBJECT)$v;");
    assert_eq!(t[2], Token::Identifier("OBJECT".into()));
}

// --- Hex integer literals ---

/// Verifies PHP 8 T_NAME_QUALIFIED tokenization: a reserved word touched by `\` inside a
/// qualified name becomes an identifier segment, while a lone keyword stays reserved.
#[test]
fn test_keyword_segments_in_qualified_names() {
    let id = |s: &str| Token::Identifier(s.to_string());
    let t = tokens("<?php A\\Default\\X \\List Match\\Y");
    assert_eq!(
        &t[1..10],
        &[
            id("A"), Token::Backslash, id("Default"), Token::Backslash, id("X"),
            Token::Backslash, id("List"), id("Match"), Token::Backslash,
        ]
    );
    // Lone keywords, a separator followed by whitespace, and `namespace\X` stay keywords.
    let t = tokens("<?php new Default; A\\ Default; Default\\ X; namespace\\f");
    assert!(t.contains(&Token::New));
    assert_eq!(t.iter().filter(|t| **t == Token::Default).count(), 3);
    assert!(t.contains(&Token::Namespace));
    // Fully qualified literal and predefined constants keep their dedicated tokens.
    let t = tokens("<?php \\true; \\PHP_EOL; A\\PHP_EOL");
    assert!(t.contains(&Token::True));
    assert_eq!(t.iter().filter(|t| **t == Token::PhpEol).count(), 2);
    // A sole `\self` / `\parent` / `\static` segment keeps its keyword token (PHP resolves
    // `\static::class` like `static::class`); inside a longer name it is a segment.
    let t = tokens(r"<?php \static; \self; \parent; A\static; \static\X");
    assert_eq!(t.iter().filter(|t| **t == Token::Static).count(), 1);
    assert!(t.contains(&Token::Self_) && t.contains(&Token::Parent));
    assert_eq!(t.iter().filter(|t| **t == Token::Identifier("static".into())).count(), 2);
}
