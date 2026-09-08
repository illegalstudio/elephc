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
/// Inference for operators: binary ops, instanceof, closures, pipe, and expr calls.
mod objects;
/// Inference for objects: property access, method calls, constructors, and class constants.
mod ops;
/// Syntactic inference: lightweight AST-to-PhpType rules for literal-based return type hints.
/// Used as a fallback when precise type data is unavailable (e.g., unknown callables).
pub(super) mod syntactic;

pub use syntactic::{infer_expr_type_syntactic, infer_return_type_syntactic};
