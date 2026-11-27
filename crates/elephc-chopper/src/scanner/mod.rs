//! Purpose:
//! Scans PHP source into byte-preserving tokens and lexical diagnostics.
//!
//! Called from:
//! - The compiler, Magician, and the shared PHP tokenizer API.
//!
//! Key details:
//! - Zend token boundaries and IDs come from the shared PHP 8.5 token catalog.
//! - All tokens and diagnostic offsets refer to the original source buffer.
//! - No operation launches PHP, reads files, or retains a borrowed source buffer.

use std::collections::HashMap;
use std::sync::OnceLock;

mod keywords;
mod lex;
mod lexeme;
mod names;
mod numbers;
mod operators;
mod strings;
pub use keywords::is_label_start as identifier_start;
pub(crate) use keywords::{cast_diagnostic_text, heredoc_start_header, keyword_diagnostic_spelling};
pub use lex::{LexError, LexWarning};
pub use lexeme::{compiler_trivia_len, digits_len, identifier_len};
pub(crate) use lex::{scan_compiler, CompilerLexeme, CompilerOutput};
pub(crate) use numbers::NumberKind;
pub use names::token_name;
pub use operators::{operator, Operator};
pub use strings::{
    braced_expression_len, braced_interpolation, decode_single_quoted, heredoc_closing,
    heredoc_opening, quoted_len, BracedInterpolation, HeredocOpening, HeredocOpeningError, HeredocProfile,
    InterpolationComment, InterpolationError,
};

/// One token's identifier and original source coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// PHP token identifier, or a single byte's unsigned value.
    pub id: i64,
    /// Zero-based byte offset, including any opening PHP tag or inline HTML.
    pub pos: usize,
    /// Byte length in the original input.
    pub len: usize,
    /// One-based starting line, counting CR, LF and CRLF like Zend.
    pub line: usize,
}

/// One non-fatal diagnostic with its original source location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    /// PHP's exact parser-facing message, without a "Deprecated: " prefix.
    pub message: String,
    /// One-based source line.
    pub line: usize,
    /// Zero-based byte offset.
    pub pos: usize,
    /// PHP error level Zend raises for this diagnostic (`E_WARNING` = 2 or
    /// `E_DEPRECATED` = 8192).
    pub severity: i64,
}

/// A successful scan with its non-fatal diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenizeOutput {
    /// Tokens in source order.
    pub tokens: Vec<Token>,
    /// Non-fatal diagnostics emitted by the requested tokenization mode.
    pub warnings: Vec<Warning>,
}

/// An error reported by a checked tokenizer entry point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// PHP-style error message, without a filename suffix.
    pub message: String,
    /// One-based source line at which parsing failed.
    pub line: usize,
    /// Zero-based source byte offset at which parsing failed.
    pub pos: usize,
    /// Non-fatal diagnostics PHP emits before raising the error.
    pub warnings: Vec<Warning>,
}

/// Message PHP uses for `__halt_compiler()` outside the outermost scope.
///
/// TOKEN_PARSE carries this text without an exception class. Callers that throw map this
/// exact message to `CompileError` and every other diagnostic to `ParseError`.
pub const HALT_COMPILER_OUTERMOST_SCOPE_MESSAGE: &str =
    "__HALT_COMPILER() can only be used from the outermost scope";

impl ParseError {
    /// Reports whether this diagnostic is PHP's outermost-scope `CompileError`.
    pub fn is_compile_error(&self) -> bool {
        self.message == HALT_COMPILER_OUTERMOST_SCOPE_MESSAGE
    }
}

/// Tokens and lexical diagnostics before applying tokenization-mode policies.
pub struct Scan {
    /// Tokens in source order.
    pub tokens: Vec<Token>,
    /// Lexical errors that matter to syntax validation.
    pub errors: Vec<LexError>,
    /// Lexical warnings with their original offsets.
    pub warnings: Vec<LexWarning>,
}

/// Scans byte-preserving PHP tokens and collects lexical diagnostics.
pub fn scan(source: &[u8]) -> Result<Scan, ParseError> {
    scan_with(source, lex::scan)
}

/// Scans runtime PHP source, starting in HTML mode for files and code mode for eval fragments.
pub fn scan_runtime(source: &[u8], file: bool) -> Result<Scan, ParseError> {
    scan_with(source, |source| lex::scan_runtime(source, file))
}

/// Applies the source size limit before running either scanner entry point.
fn scan_with(
    source: &[u8],
    scan: impl FnOnce(&[u8]) -> (Vec<Token>, Vec<LexError>, Vec<LexWarning>),
) -> Result<Scan, ParseError> {
    if source.len() > u32::MAX as usize {
        return Err(ParseError {
            message: "PHP source exceeds the tokenizer size limit".into(),
            line: 1,
            pos: 0,
            warnings: Vec::new(),
        });
    }
    let (tokens, errors, warnings) = scan(source);
    Ok(Scan { tokens, errors, warnings })
}

/// Tokenizes PHP source without syntax validation and keeps lexical warnings.
pub fn tokenize(source: &[u8]) -> Result<TokenizeOutput, ParseError> {
    let Scan { tokens, warnings, .. } = scan(source)?;
    // Octal-escape warnings exist without TOKEN_PARSE. Cast deprecations are parse-only.
    let warnings = warnings
        .into_iter()
        .filter(|warning| !warning.parse_only)
        .collect();
    Ok(TokenizeOutput { tokens, warnings: attach_warning_lines(source, warnings) })
}

/// Assigns one-based source lines to warnings in a single forward pass.
///
/// The lexer and the validator both report byte offsets, so recomputing each line from the
/// start of the source turned a file with many deprecated casts into quadratic work.
pub fn attach_warning_lines(source: &[u8], mut warnings: Vec<LexWarning>) -> Vec<Warning> {
    warnings.sort_by_key(|warning| warning.pos);
    let mut line = 1usize;
    let mut cursor = 0usize;
    warnings
        .into_iter()
        .map(|warning| {
            let end = warning.pos.min(source.len());
            while cursor < end {
                match source[cursor] {
                    b'\n' => line += 1,
                    b'\r' => {
                        line += 1;
                        // A CRLF pair is one line break.
                        if source.get(cursor + 1) == Some(&b'\n') {
                            cursor += 1;
                        }
                    }
                    _ => {}
                }
                cursor += 1;
            }
            Warning {
                message: warning.message,
                line,
                pos: warning.pos,
                severity: warning.severity,
            }
        })
        .collect()
}

/// Resolves one token constant from the authoritative shared catalog.
pub(crate) fn id(name: &str) -> i64 {
    *TOKEN_IDS
        .get_or_init(|| {
            elephc_builtin_contract::constants()
                .iter()
                .filter(|constant| constant.module == elephc_builtin_contract::PhpModule::Tokenizer)
                .filter_map(|constant| match constant.value {
                    elephc_builtin_contract::ConstValue::Int(value) => {
                        Some((constant.name, value))
                    }
                    _ => None,
                })
                .collect()
        })
        .get(name)
        .unwrap_or_else(|| panic!("missing tokenizer constant {name}"))
}

/// Immutable tokenizer-name to identifier index, built once per process.
static TOKEN_IDS: OnceLock<HashMap<&'static str, i64>> = OnceLock::new();

/// Counts logical PHP newlines, treating a CRLF pair as one line break.
pub fn newline_count(bytes: &[u8]) -> usize {
    if bytes.len() <= 1 {
        return usize::from(bytes.first().is_some_and(|byte| matches!(byte, b'\n' | b'\r')));
    }
    let mut count = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => count += 1,
            b'\r' => {
                count += 1;
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    count
}

/// Raw scanning keeps malformed source bytes and records the error for the validator.
#[cfg(test)]
#[test]
fn malformed_php_remains_tokenizable_without_a_parser() {
    let source = b"<?php \xff /* unterminated";
    let scanned = scan(source).expect("lexical errors are collected separately");
    assert!(!scanned.errors.is_empty());

    let output = tokenize(source).expect("standalone tokenization accepts malformed source");
    assert_eq!(output.tokens, scanned.tokens);
    let bytes: Vec<_> = output
        .tokens
        .iter()
        .flat_map(|token| source[token.pos..token.pos + token.len].iter().copied())
        .collect();
    assert_eq!(bytes, source);
}

/// Verifies the single-byte fast path and CRLF pairing in `newline_count`.
#[cfg(test)]
#[test]
fn newline_count_counts_lf_and_crlf_like_zend() {
    assert_eq!(newline_count(b""), 0);
    assert_eq!(newline_count(b"\n"), 1);
    assert_eq!(newline_count(b"\r"), 1);
    assert_eq!(newline_count(b"\r\n"), 1);
    assert_eq!(newline_count(b"a\r\nb\nc\r"), 3);
}
