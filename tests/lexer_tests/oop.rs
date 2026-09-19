//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of object-oriented PHP, including lex double colon, and lex this.
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

// --- Spaceship operator ---


/// Verifies a declarator list reaches the parser as ONE modifier run with commas between names.
///
/// The parsers loop over the comma (issue #684), so the lexer has to hand them exactly one
/// `Public`/`Int` prefix and then `Variable, Comma, Variable` — not a repeated modifier run. A
/// `Comma` appearing anywhere else in the member, or a second `Public`, would mean the loop is
/// reading a different shape than the source wrote.
#[test]
fn test_lex_property_declarator_list() {
    let t = tokens("<?php class C { public int $w = 40, $h = 22; }");
    assert_eq!(
        t.iter().filter(|token| **token == Token::Public).count(),
        1,
        "the modifier belongs to the whole list, not to each name"
    );
    assert_eq!(
        t.iter().filter(|token| **token == Token::Comma).count(),
        1,
        "one comma separates the two declarators"
    );
    assert_eq!(
        t.iter()
            .filter(|token| matches!(token, Token::Variable(name) if name == "w" || name == "h"))
            .count(),
        2,
        "both names survive tokenization"
    );
}

/// Verifies a constant list tokenizes as one `Const` followed by comma-separated declarators.
#[test]
fn test_lex_class_constant_declarator_list() {
    let t = tokens("<?php class C { const A = 1, B = 2; }");
    assert_eq!(
        t.iter().filter(|token| **token == Token::Const).count(),
        1,
        "one `const` introduces the whole list"
    );
    assert_eq!(
        t.iter().filter(|token| **token == Token::Comma).count(),
        1
    );
    assert_eq!(
        t.iter().filter(|token| **token == Token::Assign).count(),
        2,
        "each declarator carries its own value"
    );
}
