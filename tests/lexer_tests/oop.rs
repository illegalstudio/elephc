//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of object-oriented PHP, including lex double colon, lex this, and lex clone.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

/// Verifies `::` (double colon) tokenizes as `DoubleColon` for static access.
#[test]
fn test_lex_double_colon() {
    let t = tokens("<?php Point::origin();");
    assert!(t.contains(&Token::DoubleColon));
}

/// Verifies `$this` tokenizes as `This`.
#[test]
fn test_lex_this() {
    let t = tokens("<?php $this->value;");
    assert_eq!(t[1], Token::This);
}

/// Verifies the `clone` keyword tokenizes as `Token::Clone` for the clone expression.
#[test]
fn test_lex_clone() {
    let t = tokens("<?php $b = clone $a;");
    // OpenTag, Variable("b"), Equals, Clone, Variable("a"), Semicolon
    assert_eq!(t[3], Token::Clone);
}

// --- Spaceship operator ---
