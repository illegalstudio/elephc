//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of operators, including arithmetic operators, assignment and dot, and comparison operators.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

// Tokenizes `source` and returns only the `Token` values, discarding spans.
// Uses the parent module's `tokens` helper so tests receive bare `Token`
// entries consistent with other lexer submodules.

// Verifies `+`, `-`, `*`, `/`, `%` tokenize as distinct arithmetic operators.
#[test]
fn test_arithmetic_operators() {
    let t = tokens("<?php + - * / %");
    assert_eq!(
        t[1..6],
        [Token::Plus, Token::Minus, Token::Star, Token::Slash, Token::Percent]
    );
}

// Verifies `.` and `=` tokenize as concat and assignment respectively.
#[test]
fn test_assignment_and_dot() {
    let t = tokens("<?php . =");
    assert_eq!(t[1..3], [Token::Dot, Token::Assign]);
}

// Verifies `==`, `!=`, `<`, `>`, `<=`, `>=` tokenize as comparison operators.
#[test]
fn test_comparison_operators() {
    let t = tokens("<?php == != < > <= >=");
    assert_eq!(
        t[1..7],
        [
            Token::EqualEqual,
            Token::NotEqual,
            Token::Less,
            Token::Greater,
            Token::LessEqual,
            Token::GreaterEqual,
        ]
    );
}

// Verifies `&&`, `||`, `and`, `or`, `xor` tokenize as logical operators.
#[test]
fn test_logical_operators() {
    let t = tokens("<?php && || and or xor");
    assert_eq!(
        t[1..6],
        [Token::AndAnd, Token::OrOr, Token::And, Token::Or, Token::Xor]
    );
}

// Verifies `and`, `or`, `xor` are case-insensitive (`AND`, `Or`, `xOr` all valid).
#[test]
fn test_word_logical_operators_are_case_insensitive() {
    let t = tokens("<?php AND Or xOr");
    assert_eq!(t[1..4], [Token::And, Token::Or, Token::Xor]);
}

// Verifies `!` tokenizes as `Bang`.
#[test]
fn test_bang() {
    let t = tokens("<?php !");
    assert_eq!(t[1], Token::Bang);
}

// Verifies compound assignment operators (`+=`, `-=`, `*=`, `**=`, etc.) tokenize correctly.
#[test]
fn test_compound_assignment() {
    let t = tokens("<?php += -= *= **= /= .= %= &= |= ^= <<= >>=");
    assert_eq!(
        t[1..13],
        [
            Token::PlusAssign,
            Token::MinusAssign,
            Token::StarAssign,
            Token::StarStarAssign,
            Token::SlashAssign,
            Token::DotAssign,
            Token::PercentAssign,
            Token::AmpAssign,
            Token::PipeAssign,
            Token::CaretAssign,
            Token::LessLessAssign,
            Token::GreaterGreaterAssign,
        ]
    );
}

// Verifies `++` and `--` tokenize as increment/decrement operators.
#[test]
fn test_increment_decrement() {
    let t = tokens("<?php ++ --");
    assert_eq!(t[1..3], [Token::PlusPlus, Token::MinusMinus]);
}

// Verifies array subscript access `$a[0]` tokenizes correctly with brackets.
#[test]
fn test_array_subscript_brackets() {
    let t = tokens("<?php $a[0];");
    assert_eq!(
        t[1..6],
        [
            Token::Variable("a".into()),
            Token::LBracket,
            Token::IntLiteral(0),
            Token::RBracket,
            Token::Semicolon,
        ]
    );
}

// Verifies a complete assignment statement (`$x = 42;`) tokenizes with expected sequence.
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

// Verifies `===` tokenizes as strict equality (`EqualEqualEqual`).
#[test]
fn test_strict_equal() {
    let t = tokens("<?php ===");
    assert_eq!(t[1], Token::EqualEqualEqual);
}

// Verifies `!==` tokenizes as strict inequality (`NotEqualEqual`).
#[test]
fn test_strict_not_equal() {
    let t = tokens("<?php !==");
    assert_eq!(t[1], Token::NotEqualEqual);
}

// Verifies `===` is distinct from `==` (no token merging).
#[test]
fn test_strict_equal_vs_loose_equal() {
    let t = tokens("<?php === ==");
    assert_eq!(t[1], Token::EqualEqualEqual);
    assert_eq!(t[2], Token::EqualEqual);
}

// Verifies `!==` is distinct from `!=` (no token merging).
#[test]
fn test_strict_not_equal_vs_loose_not_equal() {
    let t = tokens("<?php !== !=");
    assert_eq!(t[1], Token::NotEqualEqual);
    assert_eq!(t[2], Token::NotEqual);
}

// --- Include/Require ---

// Verifies `**` tokenizes as exponentiation (`StarStar`), not two stars.
#[test]
fn test_star_star() {
    let t = tokens("<?php **");
    assert_eq!(t[1], Token::StarStar);
}

// Verifies `**` vs `*` are distinct tokens with correct precedence.
#[test]
fn test_star_vs_star_star() {
    let t = tokens("<?php ** *");
    assert_eq!(t[1], Token::StarStar);
    assert_eq!(t[2], Token::Star);
}

// --- Constants ---

// Verifies `.` in string concatenation is not mistaken for a float.
#[test]
fn test_dot_operator_not_float() {
    let t = tokens("<?php \"a\" . \"b\"");
    assert_eq!(t[2], Token::Dot);
}

// --- Print keyword ---

// Verifies `&` vs `&&` are distinct tokens (bitwise vs logical AND).
#[test]
fn test_ampersand_vs_andand() {
    let t = tokens("<?php $x & $y && $z;");
    assert!(t.contains(&Token::Ampersand));
    assert!(t.contains(&Token::AndAnd));
}

// Verifies `|` vs `||` are distinct tokens (bitwise vs logical OR).
#[test]
fn test_pipe_vs_oror() {
    let t = tokens("<?php $x | $y || $z;");
    assert!(t.contains(&Token::Pipe));
    assert!(t.contains(&Token::OrOr));
}

// Verifies `->` (arrow) tokenizes correctly for property access.
#[test]
fn test_lex_arrow_operator() {
    let t = tokens("<?php $obj->prop;");
    assert!(t.contains(&Token::Arrow));
}

// Verifies `?->` (nullsafe arrow) tokenizes as `QuestionArrow`, not `Question` + `Arrow`.
#[test]
fn test_lex_nullsafe_arrow_operator() {
    let t = tokens("<?php $obj?->prop;");
    assert!(t.contains(&Token::QuestionArrow));
    assert!(!t.contains(&Token::Question));
    assert!(!t.contains(&Token::Arrow));
}

// Verifies `?` vs `??` are distinct tokens (ternary vs null coalescing).
#[test]
fn test_question_vs_question_question() {
    let t = tokens("<?php $x ? $y : $z ?? $w;");
    assert!(t.contains(&Token::Question));
    assert!(t.contains(&Token::QuestionQuestion));
}

// --- Heredoc / Nowdoc ---