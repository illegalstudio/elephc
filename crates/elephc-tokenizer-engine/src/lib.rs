//! Purpose:
//! Shared, byte-preserving PHP tokenizer engine for compiled programs and runtime eval.
//!
//! Called from:
//! - The standalone tokenizer C bridge and Magician's tokenizer builtin bindings.
//!
//! Key details:
//! - The native lexer preserves Zend boundaries; TOKEN_PARSE uses elephc's AST parser.
//! - The `parse` feature is opt-in: a lexer-only embedder links no parser, and the
//!   tokenizer bridge and Magician turn it on to keep PHP's TOKEN_PARSE semantics.
//! - The adapter preserves Zend token boundaries and uses the shared PHP 8.5 token catalog.
//! - No operation launches PHP, reads files, or retains a borrowed source buffer.

use std::collections::HashMap;
use std::sync::OnceLock;

mod keywords;
mod lex;
mod names;
#[cfg(feature = "parse")]
mod parse;
#[cfg(test)]
mod tests;

#[cfg(feature = "parse")]
pub(crate) use lex::LexError;

pub use names::token_name;

/// Enables parser validation and reserved-word feedback, as PHP's TOKEN_PARSE does.
///
/// Only available when the `parse` feature is enabled; a lexer-only build rejects it.
pub const TOKEN_PARSE: i64 = 1;

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

/// One non-fatal diagnostic PHP raises while scanning with TOKEN_PARSE.
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
    /// Non-fatal diagnostics raised by TOKEN_PARSE.
    pub warnings: Vec<Warning>,
}

/// A catchable ParseError from TOKEN_PARSE validation.
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

/// Splits source into PHP tokens, optionally validating its PHP syntax.
pub fn tokenize(source: &[u8], flags: i64) -> Result<Vec<Token>, ParseError> {
    tokenize_with_warnings(source, flags).map(|output| output.tokens)
}

/// Splits source into tokens and keeps TOKEN_PARSE's non-fatal diagnostics.
pub fn tokenize_with_warnings(source: &[u8], flags: i64) -> Result<TokenizeOutput, ParseError> {
    if source.len() > u32::MAX as usize {
        return Err(ParseError {
            message: "PHP source exceeds the tokenizer size limit".into(),
            line: 1,
            pos: 0,
            warnings: Vec::new(),
        });
    }
    #[cfg(feature = "parse")]
    let (mut tokens, lexical, mut warnings) = lex::scan(source);
    #[cfg(not(feature = "parse"))]
    let (tokens, lexical, warnings) = lex::scan(source);
    let parse = flags & TOKEN_PARSE != 0;
    if parse {
        #[cfg(feature = "parse")]
        {
            let mut lexical = lexical;
            lexical.sort_by_key(|error| error.pos);
            let validation = parse::validate(source, &mut tokens, &lexical);
            let warnings = match validation {
                Ok(more) => {
                    warnings.extend(more);
                    attach_warning_lines(source, warnings)
                }
                Err(mut error) => {
                    // PHP emits only the diagnostics reached before the failure, whether the
                    // lexer or the validator raised them.
                    let failure = error.pos;
                    let mut merged: Vec<Warning> = attach_warning_lines(source, warnings)
                        .into_iter()
                        .filter(|warning| warning.pos <= failure)
                        .collect();
                    merged.extend(
                        error
                            .warnings
                            .into_iter()
                            .filter(|warning| warning.pos <= failure),
                    );
                    merged.sort_by_key(|warning| warning.pos);
                    error.warnings = merged;
                    return Err(error);
                }
            };
            return Ok(TokenizeOutput { tokens, warnings });
        }
        #[cfg(not(feature = "parse"))]
        {
            let _ = (lexical, warnings);
            return Err(ParseError {
                message: "TOKEN_PARSE requires the tokenizer engine's 'parse' feature".into(),
                line: 1,
                pos: 0,
                warnings: Vec::new(),
            });
        }
    }
    #[cfg(not(feature = "parse"))]
    let _ = lexical;
    // Octal-escape warnings exist without TOKEN_PARSE; the cast deprecations do not.
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
fn attach_warning_lines(source: &[u8], mut warnings: Vec<lex::LexWarning>) -> Vec<Warning> {
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
pub(crate) fn newline_count(bytes: &[u8]) -> usize {
    bytes.iter().enumerate().filter(|&(i, b)| *b == b'\n' || *b == b'\r' && bytes.get(i + 1) != Some(&b'\n')).count()
}
