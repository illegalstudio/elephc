//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of keywords, including packed class and buffer type tokens, interface related keywords, and keyword trait.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

#[test]
fn test_packed_class_and_buffer_type_tokens() {
    let t = tokens("<?php packed class Vec2 { public float $x; public float $y; } buffer<int> $points = buffer_new<int>(2);");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Packed,
            Token::Class,
            Token::Identifier("Vec2".into()),
            Token::LBrace,
            Token::Public,
            Token::Identifier("float".into()),
            Token::Variable("x".into()),
            Token::Semicolon,
            Token::Public,
            Token::Identifier("float".into()),
            Token::Variable("y".into()),
            Token::Semicolon,
            Token::RBrace,
            Token::Identifier("buffer".into()),
            Token::Less,
            Token::Identifier("int".into()),
            Token::Greater,
            Token::Variable("points".into()),
            Token::Assign,
            Token::Identifier("buffer_new".into()),
            Token::Less,
            Token::Identifier("int".into()),
            Token::Greater,
            Token::LParen,
            Token::IntLiteral(2),
            Token::RParen,
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_interface_related_keywords() {
    let t = tokens("<?php interface implements abstract final");
    assert_eq!(t[1], Token::Interface);
    assert_eq!(t[2], Token::Implements);
    assert_eq!(t[3], Token::Abstract);
    assert_eq!(t[4], Token::Final);
}

#[test]
fn test_keyword_trait() {
    assert_eq!(tokens("<?php trait")[1], Token::Trait);
}

#[test]
fn test_keyword_protected() {
    assert_eq!(tokens("<?php protected")[1], Token::Protected);
}

#[test]
fn test_keyword_self() {
    assert_eq!(tokens("<?php self")[1], Token::Self_);
}

#[test]
fn test_keyword_insteadof() {
    assert_eq!(tokens("<?php insteadof")[1], Token::InsteadOf);
}

#[test]
fn test_lex_class_keyword() {
    let t = tokens("<?php class Point { public $x; private $y; public readonly $id; } $p = new Point();");
    assert!(t.contains(&Token::Class));
    assert!(t.contains(&Token::New));
    assert!(t.contains(&Token::Public));
    assert!(t.contains(&Token::Private));
    assert!(t.contains(&Token::ReadOnly));
}

#[test]
fn test_lex_typed_property_tokens() {
    let t = tokens("<?php class User { public ?string $email = null; final public int $id; }");
    assert!(t.contains(&Token::Class));
    assert!(t.contains(&Token::Public));
    assert!(t.contains(&Token::Question));
    assert!(t.contains(&Token::Identifier("string".into())));
    assert!(t.contains(&Token::Variable("email".into())));
    assert!(t.contains(&Token::Null));
    assert!(t.contains(&Token::Final));
    assert!(t.contains(&Token::Identifier("int".into())));
    assert!(t.contains(&Token::Variable("id".into())));
}
