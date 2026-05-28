//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of doc comment and string-adjacent tokens, including heredoc token, nowdoc token, and heredoc interpolation token.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

/// Verifies that heredoc syntax with a basic label (`<<<EOT`) tokenizes
/// the body as a single `StringLiteral` token without interpolation.
#[test]
fn test_heredoc_token() {
    let t = tokens("<?php <<<EOT\nHello\nEOT;");
    assert!(t.contains(&Token::StringLiteral("Hello".into())));
}

/// Verifies that nowdoc syntax with a single-quoted label (`<<<'EOT'`)
/// tokenizes the body as a literal `StringLiteral` token with no variable
/// interpolation, even when `$`-prefixed identifiers appear in the body.
#[test]
fn test_nowdoc_token() {
    let t = tokens("<?php <<<'EOT'\nHello\nEOT;");
    assert!(t.contains(&Token::StringLiteral("Hello".into())));
}

/// Verifies that heredoc with a variable reference (`$name`) emits separate
/// `Variable` and `StringLiteral` tokens, and that a `Dot` token separates
/// the literal from the variable in the token stream.
#[test]
fn test_heredoc_interpolation_token() {
    let t = tokens("<?php <<<EOT\nHello $name\nEOT;");
    assert!(t.contains(&Token::Variable("name".into())));
    assert!(t.contains(&Token::Dot));
    assert!(t.contains(&Token::StringLiteral("Hello ".into())));
}

/// Verifies that PHP escape sequences inside heredoc are correctly decoded:
/// `\r` → carriage return, `\x42` → hex byte, `\102` → octal byte,
/// `\u{1F600}` → UTF-8 emoji codepoint.
#[test]
fn test_heredoc_php_escape_sequences() {
    let t = tokens("<?php <<<EOT\nA\\r\\x42\\102\\u{1F600}\nEOT;");
    assert!(t.contains(&Token::StringLiteral("A\rBB😀".into())));
}

/// Verifies that nowdoc with a variable-like sequence in the body preserves it
/// as a literal `StringLiteral` and does not emit a `Variable` token.
#[test]
fn test_nowdoc_no_interpolation_token() {
    let t = tokens("<?php <<<'EOT'\nHello $name\nEOT;");
    // Nowdoc: $name stays as literal text, no Variable token
    assert!(t.contains(&Token::StringLiteral("Hello $name".into())));
    assert!(!t.contains(&Token::Variable("name".into())));
}

// --- Const keyword ---
