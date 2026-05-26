//! Purpose:
//! Provides the public lexer entry point from PHP source text to spanned tokens.
//! Keeps token definitions public while hiding cursor and scanning helpers.
//!
//! Called from:
//! - `crate::pipeline::compile()` and `crate::resolver::files::parse_file()`.
//!
//! Key details:
//! - Every emitted token carries a span used later for parser and semantic diagnostics.

mod cursor;
mod literals;
mod scan;
/// Lexer token module.
pub mod token;

pub use token::Token;

use crate::errors::CompileError;
use crate::span::Span;

/// Tokenizes PHP source text into a stream of spanned tokens.
///
/// Entry point for the lexer pipeline. Requires the source to begin with `<?php`.
/// Each token carries a `Span` (line, column) used for parser diagnostics.
///
/// Returns `Err` if the source is missing the opening `<?php` tag or contains
/// an unterminated string literal.
pub fn tokenize(source: &str) -> Result<Vec<(Token, Span)>, CompileError> {
    scan::scan_tokens(source)
}
