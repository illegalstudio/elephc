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
}

/// Splits source into PHP tokens, optionally validating its PHP syntax.
pub fn tokenize(source: &[u8], flags: i64) -> Result<Vec<Token>, ParseError> {
    tokenize_with_warnings(source, flags).map(|output| output.tokens)
}

/// Splits source into tokens and keeps TOKEN_PARSE's non-fatal diagnostics.
pub fn tokenize_with_warnings(source: &[u8], flags: i64) -> Result<TokenizeOutput, ParseError> {
    if source.len() > u32::MAX as usize {
        return Err(ParseError { message: "PHP source exceeds the tokenizer size limit".into(), line: 1 });
    }
    #[cfg(feature = "parse")]
    let (mut tokens, lexical, warnings) = lex::scan(source);
    #[cfg(not(feature = "parse"))]
    let (tokens, lexical, warnings) = lex::scan(source);
    if flags & TOKEN_PARSE != 0 {
        #[cfg(feature = "parse")]
        {
            let mut lexical = lexical;
            lexical.sort_by_key(|error| error.pos);
            parse::validate(source, &mut tokens, &lexical)?;
            return Ok(TokenizeOutput { tokens, warnings: warnings.into_iter().map(|warning| Warning {
                message: warning.message,
                line: 1 + newline_count(&source[..warning.pos.min(source.len())]),
                pos: warning.pos,
            }).collect() });
        }
        #[cfg(not(feature = "parse"))]
        {
            let _ = (lexical, warnings);
            return Err(ParseError {
                message: "TOKEN_PARSE requires the tokenizer engine's 'parse' feature".into(),
                line: 1,
            });
        }
    }
    #[cfg(not(feature = "parse"))]
    let _ = (lexical, warnings);
    Ok(TokenizeOutput { tokens, warnings: Vec::new() })
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
