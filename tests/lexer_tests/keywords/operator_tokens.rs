//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of operator tokens, including double arrow token, ampersand token, and pipe token.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

#[test]
fn test_double_arrow_token() {
    let t = tokens("<?php [1 => 2];");
    assert!(t.contains(&Token::DoubleArrow));
}

#[test]
fn test_ampersand_token() {
    let t = tokens("<?php $x & $y;");
    assert!(t.contains(&Token::Ampersand));
}

#[test]
fn test_pipe_token() {
    let t = tokens("<?php $x | $y;");
    assert!(t.contains(&Token::Pipe));
}

#[test]
fn test_caret_token() {
    let t = tokens("<?php $x ^ $y;");
    assert!(t.contains(&Token::Caret));
}

#[test]
fn test_tilde_token() {
    let t = tokens("<?php ~$x;");
    assert!(t.contains(&Token::Tilde));
}

#[test]
fn test_shift_left_token() {
    let t = tokens("<?php $x << $y;");
    assert!(t.contains(&Token::LessLess));
}

#[test]
fn test_shift_right_token() {
    let t = tokens("<?php $x >> $y;");
    assert!(t.contains(&Token::GreaterGreater));
}

#[test]
fn test_spaceship_token() {
    let t = tokens("<?php $x <=> $y;");
    assert!(t.contains(&Token::Spaceship));
}

// --- Null coalescing operator ---

#[test]
fn test_question_question_token() {
    let t = tokens("<?php $x ?? $y;");
    assert!(t.contains(&Token::QuestionQuestion));
}

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

#[test]
fn test_pipe_arrow_token() {
    let t = tokens("<?php $x |> $y;");
    assert!(t.contains(&Token::PipeArrow));
}

#[test]
fn test_pipe_arrow_does_not_shadow_bitwise_pipe() {
    let t = tokens("<?php $x | $y;");
    assert!(t.contains(&Token::Pipe));
    assert!(!t.contains(&Token::PipeArrow));
}

#[test]
fn test_pipe_arrow_does_not_shadow_pipe_assign() {
    let t = tokens("<?php $x |= $y;");
    assert!(t.contains(&Token::PipeAssign));
    assert!(!t.contains(&Token::PipeArrow));
}

#[test]
fn test_pipe_arrow_does_not_shadow_or_or() {
    let t = tokens("<?php $x || $y;");
    assert!(t.contains(&Token::OrOr));
    assert!(!t.contains(&Token::PipeArrow));
}

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
