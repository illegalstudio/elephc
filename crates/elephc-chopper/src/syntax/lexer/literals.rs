//! Purpose:
//! Groups literal and identifier scanners behind the lexer module boundary.
//! Re-exports focused scanners for identifiers, numeric forms, and string syntaxes.
//!
//! Called from:
//! - `crate::syntax::lexer::scan` during token recognition.
//!
//! Key details:
//! - Literal scanners share cursor state and must leave it positioned after the consumed token.

mod identifiers;
mod numbers;
mod strings;

pub(super) use identifiers::variable_token;
pub(crate) use identifiers::word_token;
pub(super) use numbers::number_token;
pub(super) use strings::{decode_double_quoted_body, scan_heredoc, single_quoted_token};
