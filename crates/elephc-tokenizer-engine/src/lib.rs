//! Purpose:
//! Shared, byte-preserving PHP tokenizer engine for compiled programs and runtime eval.
//!
//! Called from:
//! - The standalone tokenizer C bridge and Magician's tokenizer builtin bindings.
//!
//! Key details:
//! - Mago provides the PHP grammar independently of elephc's compilable language subset.
//! - The adapter preserves Zend token boundaries and uses the shared PHP 8.5 token catalog.
//! - No operation launches PHP, reads files, or retains a borrowed source buffer.

mod lex;
mod names;
mod parse;
#[cfg(test)]
mod tests;

pub use names::token_name;

/// Enables parser validation and reserved-word feedback, as PHP's TOKEN_PARSE does.
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
    if source.len() > u32::MAX as usize {
        return Err(ParseError { message: "PHP source exceeds the tokenizer size limit".into(), line: 1 });
    }
    let mut tokens = lex::tokenize(source);
    if flags & TOKEN_PARSE != 0 {
        parse::validate(source, &mut tokens)?;
    }
    Ok(tokens)
}

/// Resolves one token constant from the authoritative shared catalog.
pub(crate) fn id(name: &str) -> i64 {
    match elephc_builtin_contract::lookup_constant(name).map(|c| c.value) {
        Some(elephc_builtin_contract::ConstValue::Int(value)) => value,
        _ => panic!("missing tokenizer constant {name}"),
    }
}

/// Counts logical PHP newlines, treating a CRLF pair as one line break.
pub(crate) fn newline_count(bytes: &[u8]) -> usize {
    bytes.iter().enumerate().filter(|&(i, b)| *b == b'\n' || *b == b'\r' && bytes.get(i + 1) != Some(&b'\n')).count()
}
