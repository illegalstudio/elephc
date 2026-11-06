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

mod inline_html;
mod scan;
mod strings;
#[cfg(test)]
mod tests;
mod token;

pub(crate) use scan::{tokenize, tokenize_at_line};
pub(crate) use token::{Token, TokenKind};
