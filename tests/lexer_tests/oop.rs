//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of object-oriented PHP, including lex double colon, and lex this.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

#[test]
fn test_lex_double_colon() {
    let t = tokens("<?php Point::origin();");
    assert!(t.contains(&Token::DoubleColon));
}

#[test]
fn test_lex_this() {
    let t = tokens("<?php $this->value;");
    assert_eq!(t[1], Token::This);
}

// --- Spaceship operator ---
