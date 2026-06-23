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

mod scan;
mod token;

pub(crate) use scan::tokenize;
pub(crate) use token::TokenKind;
