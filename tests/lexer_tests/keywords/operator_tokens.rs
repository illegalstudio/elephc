//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of operator tokens, including double arrow token, ampersand token, and pipe token.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

/// Verifies `=>` (double arrow) tokenizes in array context.
#[test]
fn test_double_arrow_token() {
    let t = tokens("<?php [1 => 2];");
    assert!(t.contains(&Token::DoubleArrow));
}

/// Verifies `&` (bitwise AND) tokenizes as `Ampersand`.
#[test]
fn test_ampersand_token() {
    let t = tokens("<?php $x & $y;");
    assert!(t.contains(&Token::Ampersand));
}

/// Verifies `|` (bitwise OR) tokenizes as `Pipe`.
#[test]
fn test_pipe_token() {
    let t = tokens("<?php $x | $y;");
    assert!(t.contains(&Token::Pipe));
}

/// Verifies `^` (bitwise XOR) tokenizes as `Caret`.
#[test]
fn test_caret_token() {
    let t = tokens("<?php $x ^ $y;");
    assert!(t.contains(&Token::Caret));
}

/// Verifies `~` (bitwise NOT) tokenizes as `Tilde`.
#[test]
fn test_tilde_token() {
    let t = tokens("<?php ~$x;");
    assert!(t.contains(&Token::Tilde));
}

/// Verifies `<<` (left shift) tokenizes as `LessLess`.
#[test]
fn test_shift_left_token() {
    let t = tokens("<?php $x << $y;");
    assert!(t.contains(&Token::LessLess));
}

/// Verifies `>>` (right shift) tokenizes as `GreaterGreater`.
#[test]
fn test_shift_right_token() {
    let t = tokens("<?php $x >> $y;");
    assert!(t.contains(&Token::GreaterGreater));
}

/// Verifies `<=>` (spaceship) tokenizes as `Spaceship`.
#[test]
fn test_spaceship_token() {
    let t = tokens("<?php $x <=> $y;");
    assert!(t.contains(&Token::Spaceship));
}

// --- Null coalescing operator ---

/// Verifies `??` (null coalescing) tokenizes as `QuestionQuestion`.
#[test]
fn test_question_question_token() {
    let t = tokens("<?php $x ?? $y;");
    assert!(t.contains(&Token::QuestionQuestion));
}

/// Verifies `??=` (null coalescing assign) tokenizes as `QuestionQuestionAssign`.
#[test]
fn test_question_question_assign_token() {
    let t = tokens("<?php $x ??= $y;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Variable("x".into()),
            Token::QuestionQuestionAssign,
            Token::Variable("y".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

// --- PHP 8.5 pipe operator ---

/// Verifies `|>` (pipe arrow, PHP 8.5) tokenizes as `PipeArrow`.
#[test]
fn test_pipe_arrow_token() {
    let t = tokens("<?php $x |> $y;");
    assert!(t.contains(&Token::PipeArrow));
}

/// Verifies `|>` does not shadow bitwise `|` when both appear.
#[test]
fn test_pipe_arrow_does_not_shadow_bitwise_pipe() {
    let t = tokens("<?php $x | $y;");
    assert!(t.contains(&Token::Pipe));
    assert!(!t.contains(&Token::PipeArrow));
}

/// Verifies `|>` does not shadow `|=` compound assignment.
#[test]
fn test_pipe_arrow_does_not_shadow_pipe_assign() {
    let t = tokens("<?php $x |= $y;");
    assert!(t.contains(&Token::PipeAssign));
    assert!(!t.contains(&Token::PipeArrow));
}

/// Verifies `|>` does not shadow `||` logical OR.
#[test]
fn test_pipe_arrow_does_not_shadow_or_or() {
    let t = tokens("<?php $x || $y;");
    assert!(t.contains(&Token::OrOr));
    assert!(!t.contains(&Token::PipeArrow));
}

/// Verifies `|>` without spaces still tokenizes correctly.
#[test]
fn test_pipe_arrow_without_spaces() {
    let t = tokens("<?php $x|>$y;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Variable("x".into()),
            Token::PipeArrow,
            Token::Variable("y".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}
