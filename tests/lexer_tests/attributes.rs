//! Purpose:
//! Lexer regression tests for PHP attribute open tokens and `#` comments.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `#[` must tokenize as `AttrOpen` outside string/heredoc literals.
//! - Bare `#` starts a PHP-style line comment.

use super::*;

#[test]
fn test_attribute_open_token() {
    // `#[` is a single token; the following identifier and `]` lex normally.
    let t = tokens("<?php #[Foo] class C {}");
    assert_eq!(t[1], Token::AttrOpen);
    assert_eq!(t[2], Token::Identifier("Foo".into()));
    assert_eq!(t[3], Token::RBracket);
    assert_eq!(t[4], Token::Class);
}

#[test]
fn test_attribute_with_arguments() {
    let t = tokens("<?php #[Bar(1, \"two\")]");
    assert_eq!(t[1], Token::AttrOpen);
    assert_eq!(t[2], Token::Identifier("Bar".into()));
    assert_eq!(t[3], Token::LParen);
    assert_eq!(t[4], Token::IntLiteral(1));
    assert_eq!(t[5], Token::Comma);
    assert_eq!(t[6], Token::StringLiteral("two".into()));
    assert_eq!(t[7], Token::RParen);
    assert_eq!(t[8], Token::RBracket);
}

#[test]
fn test_multiple_attributes_in_one_group() {
    let t = tokens("<?php #[A, B(1)]");
    assert_eq!(t[1], Token::AttrOpen);
    assert_eq!(t[2], Token::Identifier("A".into()));
    assert_eq!(t[3], Token::Comma);
    assert_eq!(t[4], Token::Identifier("B".into()));
}

#[test]
fn test_php_hash_line_comment_is_skipped() {
    // `# ...` is a PHP line comment when not followed by `[`. The lexer must
    // skip it without producing any tokens for the comment text.
    let t = tokens("<?php # this is a comment\necho 1;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::IntLiteral(1),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_hash_immediately_followed_by_bracket_is_attribute() {
    // No space between `#` and `[` must still produce AttrOpen, not a
    // line comment.
    let t = tokens("<?php #[X]\necho 1;");
    assert_eq!(t[1], Token::AttrOpen);
    assert_eq!(t[2], Token::Identifier("X".into()));
    assert_eq!(t[3], Token::RBracket);
    assert_eq!(t[4], Token::Echo);
}

#[test]
fn test_qualified_attribute_name() {
    // PHP allows fully-qualified attribute names like `#[\Symfony\Required]`.
    let t = tokens("<?php #[\\Symfony\\Required]");
    assert_eq!(t[1], Token::AttrOpen);
    assert_eq!(t[2], Token::Backslash);
    assert_eq!(t[3], Token::Identifier("Symfony".into()));
    assert_eq!(t[4], Token::Backslash);
    assert_eq!(t[5], Token::Identifier("Required".into()));
    assert_eq!(t[6], Token::RBracket);
}

#[test]
fn test_hash_bracket_inside_double_quoted_string_is_literal() {
    // The lexer scans string literals atomically — `#[` inside a string is
    // just text and must NOT produce an AttrOpen token.
    let t = tokens("<?php $s = \"look ma #[NotAttr]\";");
    let attr_count = t.iter().filter(|t| matches!(t, Token::AttrOpen)).count();
    assert_eq!(attr_count, 0, "no AttrOpen should appear inside a string");
    let s_count = t
        .iter()
        .filter(|t| matches!(t, Token::StringLiteral(s) if s.contains("#[")))
        .count();
    assert_eq!(s_count, 1, "string literal must contain the literal `#[` text");
}

#[test]
fn test_hash_bracket_inside_heredoc_is_literal() {
    // Heredoc body is also scanned atomically — `#[` is not an attribute.
    let src = "<?php $s = <<<EOT\nthis #[NotAttr] is text\nEOT;\n";
    let t = tokens(src);
    let attr_count = t.iter().filter(|t| matches!(t, Token::AttrOpen)).count();
    assert_eq!(attr_count, 0, "no AttrOpen should appear inside a heredoc");
}

#[test]
fn test_hash_bracket_inside_single_quoted_string_is_literal() {
    let t = tokens("<?php $s = 'see #[here]';");
    let attr_count = t.iter().filter(|t| matches!(t, Token::AttrOpen)).count();
    assert_eq!(attr_count, 0);
}

#[test]
fn test_hash_at_end_of_file_does_not_panic() {
    // A bare `#` with no newline and no following `[` was a corner case for
    // the lexer's comment loop — must produce a clean token stream.
    let t = tokens("<?php echo 1; #");
    let last_real = t
        .iter()
        .rev()
        .find(|t| !matches!(t, Token::Eof))
        .expect("at least one token");
    assert!(matches!(last_real, Token::Semicolon));
}

#[test]
fn test_hash_followed_by_space_and_bracket_is_comment() {
    // `# [Foo]` (with a space) must be treated as a line comment, not an
    // attribute group — PHP requires no space between `#` and `[`.
    let t = tokens("<?php # [NotAttr]\necho 1;");
    let attr_count = t.iter().filter(|t| matches!(t, Token::AttrOpen)).count();
    assert_eq!(attr_count, 0);
}
