//! Purpose:
//! Groups tokenization for runtime PHP eval fragments.
//! The lexer owns token definitions and source scanning before parser grammar
//! state consumes those tokens.
//!
//! Called from:
//! - `crate::parser::parse_fragment()`.
//!
//! Key details:
//! - Fragment line metadata is captured while scanning magic constants.
//! - PHP opening tags are rejected before tokenization by the parser entry point.
//! - Double-quoted literal interpolation lives in `strings` and rewrites one literal into
//!   a concatenation token stream the existing grammar already understands.

mod decode;
mod shared;
mod strings;
#[cfg(test)]
mod tests;
mod token;

use crate::errors::EvalParseError;

#[cfg(test)]
pub(crate) use shared::tokenize_script;
pub(crate) use shared::tokenize_script_with_semantics;
pub(crate) use token::{Token, TokenKind};

/// Tokenizes a complete runtime fragment with Chopper.
pub(crate) fn tokenize(source: &str) -> Result<Vec<Token>, EvalParseError> {
    shared::tokenize_fragment(source)
}

/// Tokenizes a runtime fragment under the selected semantic profile.
pub(crate) fn tokenize_with_semantics(
    source: &str,
    profile: elephc_semantics::SemanticProfile,
) -> Result<Vec<Token>, EvalParseError> {
    shared::tokenize_fragment_with_semantics(source, profile)
}
