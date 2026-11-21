//! Purpose:
//! Provides the shared PHP tokenizer API, including TOKEN_PARSE validation.
//!
//! Called from:
//! - The standalone tokenizer C bridge and Magician's tokenizer builtin bindings.
//!
//! Key details:
//! - Chopper scans all source bytes and supplies lexical diagnostics.
//! - The parser's validation mode supplies errors and identifier feedback.

mod parse;

use crate::scanner::{attach_warning_lines, Scan};
pub use crate::scanner::{token_name, ParseError, Token, TokenizeOutput, Warning};

/// Enables parser validation and reserved-word feedback, as PHP's TOKEN_PARSE does.
pub const TOKEN_PARSE: i64 = 1;

/// Splits source into PHP tokens, optionally validating its PHP syntax.
pub fn tokenize(source: &[u8], flags: i64) -> Result<Vec<Token>, ParseError> {
    tokenize_with_warnings(source, flags).map(|output| output.tokens)
}

/// Splits source into tokens and keeps the diagnostics emitted by the requested mode.
pub fn tokenize_with_warnings(source: &[u8], flags: i64) -> Result<TokenizeOutput, ParseError> {
    if flags & TOKEN_PARSE == 0 {
        return crate::scanner::tokenize(source);
    }

    let Scan { mut tokens, mut errors, mut warnings } = crate::scanner::scan(source)?;
    errors.sort_by_key(|error| error.pos);
    match parse::validate(source, &mut tokens, &errors) {
        Ok(more) => {
            warnings.extend(more);
            Ok(TokenizeOutput { tokens, warnings: attach_warning_lines(source, warnings) })
        }
        Err(mut error) => {
            // PHP emits only diagnostics reached before the failure.
            let failure = error.pos;
            let mut merged: Vec<Warning> = attach_warning_lines(source, warnings)
                .into_iter()
                .filter(|warning| warning.pos <= failure)
                .collect();
            merged.extend(error.warnings.into_iter().filter(|warning| warning.pos <= failure));
            merged.sort_by_key(|warning| warning.pos);
            error.warnings = merged;
            Err(error)
        }
    }
}
