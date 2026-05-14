//! Purpose:
//! Groups the keywords integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for language keywords, runtime and extension keywords, operator tokens, doc comment and string-adjacent tokens, object-oriented keywords.

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
