//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of constants, including runtime constant tokens, dir token, and file token.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

/// Verifies `PHP_EOL`, `PHP_OS`, `DIRECTORY_SEPARATOR`, `PATH_SEPARATOR` tokenize as
/// runtime constant tokens.
#[test]
fn test_runtime_constant_tokens() {
    let t = tokens("<?php PHP_EOL PHP_OS DIRECTORY_SEPARATOR PATH_SEPARATOR");
    assert_eq!(
        t[1..5],
        [
            Token::PhpEol,
            Token::PhpOs,
            Token::DirectorySeparator,
            Token::PathSeparator
        ]
    );
}

// --- STDIN / STDOUT / STDERR ---

/// Verifies `__DIR__` tokenizes as `DunderDir`.
#[test]
fn test_dunder_dir_token() {
    let t = tokens("<?php __DIR__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderDir, Token::Semicolon, Token::Eof]
    );
}

/// Verifies `__FILE__` tokenizes as `DunderFile`.
#[test]
fn test_dunder_file_token() {
    let t = tokens("<?php __FILE__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderFile, Token::Semicolon, Token::Eof]
    );
}

/// Verifies `__LINE__` tokenizes as `DunderLine`.
#[test]
fn test_dunder_line_token() {
    let t = tokens("<?php __LINE__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderLine, Token::Semicolon, Token::Eof]
    );
}

/// Verifies `__FUNCTION__` tokenizes as `DunderFunction`.
#[test]
fn test_dunder_function_token() {
    let t = tokens("<?php __FUNCTION__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderFunction, Token::Semicolon, Token::Eof]
    );
}

/// Verifies `__CLASS__` tokenizes as `DunderClass`.
#[test]
fn test_dunder_class_token() {
    let t = tokens("<?php __CLASS__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderClass, Token::Semicolon, Token::Eof]
    );
}

/// Verifies `__METHOD__` tokenizes as `DunderMethod`.
#[test]
fn test_dunder_method_token() {
    let t = tokens("<?php __METHOD__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderMethod, Token::Semicolon, Token::Eof]
    );
}

/// Verifies `__NAMESPACE__` tokenizes as `DunderNamespace`.
#[test]
fn test_dunder_namespace_token() {
    let t = tokens("<?php __NAMESPACE__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderNamespace, Token::Semicolon, Token::Eof]
    );
}

/// Verifies `__TRAIT__` tokenizes as `DunderTrait`.
#[test]
fn test_dunder_trait_token() {
    let t = tokens("<?php __TRAIT__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderTrait, Token::Semicolon, Token::Eof]
    );
}

/// Verifies `__dir__` (lowercase) still tokenizes as `DunderDir` (magic constant case-insensitive).
#[test]
fn test_dunder_lowercase_is_magic_constant() {
    let t = tokens("<?php __dir__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderDir, Token::Semicolon, Token::Eof]
    );
}

/// Verifies `__FuNcTiOn__` (mixed case) still tokenizes as `DunderFunction`.
#[test]
fn test_dunder_mixed_case_is_magic_constant() {
    let t = tokens("<?php __FuNcTiOn__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderFunction, Token::Semicolon, Token::Eof]
    );
}
