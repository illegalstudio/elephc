use super::*;

#[test]
fn test_ellipsis_token() {
    let t = tokens("<?php ...");
    assert_eq!(t, vec![Token::OpenTag, Token::Ellipsis, Token::Eof]);
}

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
