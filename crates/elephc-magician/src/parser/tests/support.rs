//! Purpose:
//! Shared imports for parser unit tests.
//! The focused test modules compare parser output against EvalIR structures
//! without repeating the parser and IR imports in each file.
//!
//! Called from:
//! - `crate::parser::tests::*` focused parser test modules.
//!
//! Key details:
//! - Re-exports are limited to parser tests through `pub(super)`.

pub(super) use super::super::cursor::inc_dec_store;
pub(super) use super::super::parse_fragment;
pub(super) use super::super::state::EVAL_YIELD_INTRINSIC;
pub(super) use crate::errors::EvalParseError;
pub(super) use crate::eval_ir::*;

/// Parses a fragment and reduces a failure to the bare parse error it reports.
///
/// A parser test asserts which grammar rule refused a fragment. The line and token that
/// `parse_fragment()` also attaches are asserted where they are built, in
/// `crate::errors` and `crate::lexer::token`, so the grammar assertions stay one-line.
pub(super) fn parse_fragment_error(code: &[u8]) -> Result<EvalProgram, EvalParseError> {
    parse_fragment(code).map_err(|diagnostic| diagnostic.error())
}
