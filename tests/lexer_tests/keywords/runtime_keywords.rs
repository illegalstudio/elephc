//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of keywords, including include keyword, include once keyword, and require keyword.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

#[test]
fn test_include_keyword() {
    let t = tokens("<?php include");
    assert_eq!(t[1], Token::Include);
}

#[test]
fn test_include_once_keyword() {
    let t = tokens("<?php include_once");
    assert_eq!(t[1], Token::IncludeOnce);
}

#[test]
fn test_require_keyword() {
    let t = tokens("<?php require");
    assert_eq!(t[1], Token::Require);
}

#[test]
fn test_require_once_keyword() {
    let t = tokens("<?php require_once");
    assert_eq!(t[1], Token::RequireOnce);
}

// --- StarStar ---

#[test]
fn test_php_int_max_token() {
    let t = tokens("<?php PHP_INT_MAX");
    assert_eq!(t[1], Token::PhpIntMax);
}

#[test]
fn test_m_pi_token() {
    let t = tokens("<?php M_PI");
    assert_eq!(t[1], Token::MPi);
}

#[test]
fn test_inf_keyword() {
    let t = tokens("<?php INF");
    assert_eq!(t[1], Token::Inf);
}

#[test]
fn test_nan_keyword() {
    let t = tokens("<?php NAN");
    assert_eq!(t[1], Token::Nan);
}

// --- Float literals ---

#[test]
fn test_stdin_token() {
    let t = tokens("<?php STDIN;");
    assert_eq!(t[1], Token::Stdin);
}

#[test]
fn test_stdout_token() {
    let t = tokens("<?php STDOUT;");
    assert_eq!(t[1], Token::Stdout);
}

#[test]
fn test_stderr_token() {
    let t = tokens("<?php STDERR;");
    assert_eq!(t[1], Token::Stderr);
}

// --- v0.6: Arrays, switch, match ---
