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

pub use token::{SpannedToken, Token, TokenMetadata};

use crate::errors::CompileError;
use crate::source::SourceMode;
/// Tokenizes PHP source text into a stream of spanned tokens.
///
/// Entry point for the lexer pipeline. Requires the source to begin with `<?php`.
/// Each token carries a `Span` (line, column) used for parser diagnostics.
///
/// Returns `Err` if the source is missing the opening `<?php` tag or contains
/// an unterminated string literal.
pub fn tokenize(source: &str) -> Result<Vec<SpannedToken>, CompileError> {
    tokenize_with_mode(source, SourceMode::Php)
}

/// Tokenizes source text according to one explicit physical-file source mode.
///
/// PHP mode requires the normal opening tag. LFC mode treats the complete
/// source as code and synthesizes the structural open-tag token consumed by
/// the shared parser, preserving the original source coordinates.
pub fn tokenize_with_mode(
    source: &str,
    mode: SourceMode,
) -> Result<Vec<SpannedToken>, CompileError> {
    scan::scan_tokens(source, mode)
}

/// Tokenizes an included physical file with a source identity carried by every span.
pub(crate) fn tokenize_with_mode_and_source_id(
    source: &str,
    mode: SourceMode,
    source_id: u32,
) -> Result<Vec<SpannedToken>, CompileError> {
    scan::scan_tokens_in_source(source, mode, source_id)
}
