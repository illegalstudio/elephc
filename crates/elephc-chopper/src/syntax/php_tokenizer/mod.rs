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

/// Returns the source offset before which PHP emits warnings for a validation failure.
///
/// An unclosed delimiter reports the opener's position while the parser actually stopped at
/// end of input; warnings reached before that stop must survive, so those failures use the
/// full source length as the cutoff. Every other failure reports the position it stopped at.
fn warning_cutoff(message: &str, pos: usize, source_len: usize) -> usize {
    if message.starts_with("Unclosed '") && !message.contains(" does not match ") {
        source_len
    } else {
        pos
    }
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
            // PHP emits only diagnostics reached before the failure. A reported position can
            // point back at the construct that was left open, so the warning cutoff stays
            // separate from the position the diagnostic highlights.
            let failure = warning_cutoff(&error.message, error.pos, source.len());
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
