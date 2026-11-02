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
//! - Double-quoted literal and heredoc interpolation lives in `strings` and rewrites one
//!   literal into a concatenation token stream the existing grammar already understands.
//! - Source bytes that are not valid UTF-8 are rewritten to private-use markers by `binary`
//!   before scanning, because PHP source is bytes and this scanner consumes Rust `str`.

mod binary;
mod inline_html;
mod scan;
mod php_tags;
mod strings;
#[cfg(test)]
mod tests;
mod token;

pub(crate) use binary::normalize_binary_source;
pub(crate) use scan::{tokenize, tokenize_at_line};
pub(crate) use php_tags::{contains_php_open_tag, find_php_close_tag};
pub(crate) use token::{Token, TokenKind};
