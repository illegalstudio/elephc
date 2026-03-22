use elephc::lexer::{tokenize, Token};

/// Helper: extract just the tokens (discard spans) for easy comparison.
fn tokens(source: &str) -> Vec<Token> {
    tokenize(source)
        .unwrap()
        .into_iter()
        .map(|(t, _)| t)
        .collect()
}

#[test]
fn test_open_tag() {
    let t = tokens("<?php");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

#[test]
fn test_echo_string() {
    let t = tokens("<?php echo \"hello\";");
    assert_eq!(
        t,
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
    let t = tokens("<?php \"hello\\nworld\\t!\"");
    assert_eq!(t[1], Token::StringLiteral("hello\nworld\t!".into()));
}

#[test]
fn test_integer_literal() {
    let t = tokens("<?php 42");
    assert_eq!(t[1], Token::IntLiteral(42));
}

#[test]
fn test_variable() {
    let t = tokens("<?php $foo");
    assert_eq!(t[1], Token::Variable("foo".into()));
}

#[test]
fn test_operators() {
    let t = tokens("<?php + - * / . =");
    assert_eq!(
        t[1..7],
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
    let t = tokens("<?php // this is a comment\necho \"hi\";");
    assert_eq!(t[1], Token::Echo);
}

#[test]
fn test_block_comment() {
    let t = tokens("<?php /* block */ echo \"hi\";");
    assert_eq!(t[1], Token::Echo);
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
    let t = tokens("<?php $x = 42;");
    assert_eq!(
        t,
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

#[test]
fn test_consecutive_comments() {
    let t = tokens("<?php /* a *//* b */// c\necho \"ok\";");
    assert_eq!(t[1], Token::Echo);
}

#[test]
fn test_span_tracking() {
    let spanned = tokenize("<?php\necho \"hi\";").unwrap();
    // "echo" starts on line 2, col 1
    let echo_span = spanned[1].1;
    assert_eq!(echo_span.line, 2);
    assert_eq!(echo_span.col, 1);
}
