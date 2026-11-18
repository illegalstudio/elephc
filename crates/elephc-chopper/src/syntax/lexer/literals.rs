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

pub(super) use identifiers::{scan_keyword, scan_variable};
pub(super) use numbers::{scan_dot_float, scan_number};
pub(super) use strings::{scan_double_string_interpolated, scan_heredoc, scan_single_string};
