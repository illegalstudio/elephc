//! Purpose:
//! Coordinates expression and syntactic type inference for the checker.
//! Routes operators, objects, callables, assignments, and lightweight AST facts through one inference entry point.
//!
//! Called from:
//! - `crate::types::checker::Checker::infer_type()`
//!
//! Key details:
//! - Inference may emit diagnostics and warnings, so callers must preserve source spans and environment context.

mod expr;
mod objects;
mod ops;
pub(super) mod syntactic;

pub use syntactic::{infer_expr_type_syntactic, infer_return_type_syntactic};
