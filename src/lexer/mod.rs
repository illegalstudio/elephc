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
mod physical;
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
    tokenize_bytes_with_mode(source.as_bytes(), mode)
}

/// Tokenizes one physical source byte stream without decoding an opaque halt payload.
///
/// Bytes before and through a valid `__HALT_COMPILER()` statement must be UTF-8,
/// while every byte after PHP's recorded halt offset is data and is never decoded.
pub fn tokenize_bytes_with_mode(
    source: &[u8],
    mode: SourceMode,
) -> Result<Vec<SpannedToken>, CompileError> {
    physical::tokenize_physical_bytes(source, mode)
}
