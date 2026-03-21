use elephc::lexer::{tokenize, Token};

#[test]
fn test_open_tag() {
    let tokens = tokenize("<?php").unwrap();
    assert_eq!(tokens[0], Token::OpenTag);
    assert_eq!(tokens[1], Token::Eof);
}

#[test]
fn test_echo_string() {
    let tokens = tokenize("<?php echo \"hello\";").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::StringLiteral("hello".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_string_escape_sequences() {
    let tokens = tokenize("<?php \"hello\\nworld\\t!\"").unwrap();
    assert_eq!(tokens[1], Token::StringLiteral("hello\nworld\t!".into()));
}

#[test]
fn test_integer_literal() {
    let tokens = tokenize("<?php 42").unwrap();
    assert_eq!(tokens[1], Token::IntLiteral(42));
}

#[test]
fn test_variable() {
    let tokens = tokenize("<?php $foo").unwrap();
    assert_eq!(tokens[1], Token::Variable("foo".into()));
}

#[test]
fn test_operators() {
    let tokens = tokenize("<?php + - * / . =").unwrap();
    assert_eq!(
        tokens[1..7],
        [
            Token::Plus,
            Token::Minus,
            Token::Star,
            Token::Slash,
            Token::Dot,
            Token::Assign,
        ]
    );
}

#[test]
fn test_line_comment() {
    let tokens = tokenize("<?php // this is a comment\necho \"hi\";").unwrap();
    assert_eq!(tokens[1], Token::Echo);
}

#[test]
fn test_block_comment() {
    let tokens = tokenize("<?php /* block */ echo \"hi\";").unwrap();
    assert_eq!(tokens[1], Token::Echo);
}

#[test]
fn test_missing_open_tag() {
    assert!(tokenize("echo \"hi\";").is_err());
}

#[test]
fn test_unterminated_string() {
    assert!(tokenize("<?php \"no closing").is_err());
}

#[test]
fn test_assignment_statement() {
    let tokens = tokenize("<?php $x = 42;").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::OpenTag,
            Token::Variable("x".into()),
            Token::Assign,
            Token::IntLiteral(42),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}
