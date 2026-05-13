//! Purpose:
//! Lexer test root wiring and shared token helper coverage for PHP tokenization suites.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Helpers tokenize inline PHP snippets while submodules assert source structure, keywords, literals, and operators.

use elephc::lexer::{tokenize, Token};

/// Helper: extract just the tokens (discard spans) for easy comparison.
fn tokens(source: &str) -> Vec<Token> {
    tokenize(source)
        .unwrap()
        .into_iter()
        .map(|(t, _)| t)
        .collect()
}

#[path = "lexer_tests/source_structure.rs"]
mod source_structure;
#[path = "lexer_tests/keywords.rs"]
mod keywords;
#[path = "lexer_tests/operators.rs"]
mod operators;
#[path = "lexer_tests/literals.rs"]
mod literals;
#[path = "lexer_tests/constants.rs"]
mod constants;
#[path = "lexer_tests/oop.rs"]
mod oop;
#[path = "lexer_tests/spread_and_calls.rs"]
mod spread_and_calls;
#[path = "lexer_tests/syntax.rs"]
mod syntax;
#[path = "lexer_tests/attributes.rs"]
mod attributes;
