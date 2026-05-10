//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of doc comment and string-adjacent tokens, including heredoc token, nowdoc token, and heredoc interpolation token.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

#[test]
fn test_heredoc_token() {
    let t = tokens("<?php <<<EOT\nHello\nEOT;");
    assert!(t.contains(&Token::StringLiteral("Hello".into())));
}

#[test]
fn test_nowdoc_token() {
    let t = tokens("<?php <<<'EOT'\nHello\nEOT;");
    assert!(t.contains(&Token::StringLiteral("Hello".into())));
}

#[test]
fn test_heredoc_interpolation_token() {
    let t = tokens("<?php <<<EOT\nHello $name\nEOT;");
    assert!(t.contains(&Token::Variable("name".into())));
    assert!(t.contains(&Token::Dot));
    assert!(t.contains(&Token::StringLiteral("Hello ".into())));
}

#[test]
fn test_nowdoc_no_interpolation_token() {
    let t = tokens("<?php <<<'EOT'\nHello $name\nEOT;");
    // Nowdoc: $name stays as literal text, no Variable token
    assert!(t.contains(&Token::StringLiteral("Hello $name".into())));
    assert!(!t.contains(&Token::Variable("name".into())));
}

// --- Const keyword ---
