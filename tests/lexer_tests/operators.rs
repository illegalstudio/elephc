//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of operators, including arithmetic operators, assignment and dot, and comparison operators.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

#[test]
fn test_arithmetic_operators() {
    let t = tokens("<?php + - * / %");
    assert_eq!(
        t[1..6],
        [Token::Plus, Token::Minus, Token::Star, Token::Slash, Token::Percent]
    );
}

#[test]
fn test_assignment_and_dot() {
    let t = tokens("<?php . =");
    assert_eq!(t[1..3], [Token::Dot, Token::Assign]);
}

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

#[test]
fn test_logical_operators() {
    let t = tokens("<?php && || and or xor");
    assert_eq!(
        t[1..6],
        [Token::AndAnd, Token::OrOr, Token::And, Token::Or, Token::Xor]
    );
}

#[test]
fn test_word_logical_operators_are_case_insensitive() {
    let t = tokens("<?php AND Or xOr");
    assert_eq!(t[1..4], [Token::And, Token::Or, Token::Xor]);
}

#[test]
fn test_bang() {
    let t = tokens("<?php !");
    assert_eq!(t[1], Token::Bang);
}

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

#[test]
fn test_increment_decrement() {
    let t = tokens("<?php ++ --");
    assert_eq!(t[1..3], [Token::PlusPlus, Token::MinusMinus]);
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
fn test_strict_equal() {
    let t = tokens("<?php ===");
    assert_eq!(t[1], Token::EqualEqualEqual);
}

#[test]
fn test_strict_not_equal() {
    let t = tokens("<?php !==");
    assert_eq!(t[1], Token::NotEqualEqual);
}

#[test]
fn test_strict_equal_vs_loose_equal() {
    let t = tokens("<?php === ==");
    assert_eq!(t[1], Token::EqualEqualEqual);
    assert_eq!(t[2], Token::EqualEqual);
}

#[test]
fn test_strict_not_equal_vs_loose_not_equal() {
    let t = tokens("<?php !== !=");
    assert_eq!(t[1], Token::NotEqualEqual);
    assert_eq!(t[2], Token::NotEqual);
}

// --- Include/Require ---

#[test]
fn test_star_star() {
    let t = tokens("<?php **");
    assert_eq!(t[1], Token::StarStar);
}

#[test]
fn test_star_vs_star_star() {
    let t = tokens("<?php ** *");
    assert_eq!(t[1], Token::StarStar);
    assert_eq!(t[2], Token::Star);
}

// --- Constants ---

#[test]
fn test_dot_operator_not_float() {
    let t = tokens("<?php \"a\" . \"b\"");
    assert_eq!(t[2], Token::Dot);
}

// --- Print keyword ---

#[test]
fn test_ampersand_vs_andand() {
    let t = tokens("<?php $x & $y && $z;");
    assert!(t.contains(&Token::Ampersand));
    assert!(t.contains(&Token::AndAnd));
}

#[test]
fn test_pipe_vs_oror() {
    let t = tokens("<?php $x | $y || $z;");
    assert!(t.contains(&Token::Pipe));
    assert!(t.contains(&Token::OrOr));
}

#[test]
fn test_lex_arrow_operator() {
    let t = tokens("<?php $obj->prop;");
    assert!(t.contains(&Token::Arrow));
}

#[test]
fn test_lex_nullsafe_arrow_operator() {
    let t = tokens("<?php $obj?->prop;");
    assert!(t.contains(&Token::QuestionArrow));
    assert!(!t.contains(&Token::Question));
    assert!(!t.contains(&Token::Arrow));
}

#[test]
fn test_question_vs_question_question() {
    let t = tokens("<?php $x ? $y : $z ?? $w;");
    assert!(t.contains(&Token::Question));
    assert!(t.contains(&Token::QuestionQuestion));
}

// --- Heredoc / Nowdoc ---
