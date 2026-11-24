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

pub(super) use super::super::parse_fragment;
pub(super) use crate::errors::EvalParseError;
pub(super) use crate::eval_ir::*;
/// Builds the expected EvalIR statement for a variable increment or decrement.
pub(super) fn inc_dec_store(name: String, increment: bool) -> EvalStmt {
    EvalStmt::IncDecVar { name, increment }
}
