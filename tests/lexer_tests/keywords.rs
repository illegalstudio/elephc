//! Purpose:
//! Wires keyword test submodules into the parent lexer suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group fixtures for language, runtime, and operator keywords;
//!   doc-comment/string-adjacent tokens; and object-oriented keywords.

use super::*;

#[path = "keywords/language_keywords.rs"]
mod language_keywords;
#[path = "keywords/runtime_keywords.rs"]
mod runtime_keywords;
#[path = "keywords/operator_tokens.rs"]
mod operator_tokens;
#[path = "keywords/doc_strings.rs"]
mod doc_strings;
#[path = "keywords/oop_keywords.rs"]
mod oop_keywords;
#[path = "keywords/yield_tokens.rs"]
mod yield_tokens;
