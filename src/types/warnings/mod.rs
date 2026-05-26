//! Purpose:
//! Runs non-fatal type-system warning analyses after core validation.
//! Aggregates unreachable-code, unused-symbol, expression-read, and OOP warning passes.
//!
//! Called from:
//! - `crate::types::check()`
//!
//! Key details:
//! - Warnings must not mutate checker semantics; they annotate already-validated programs conservatively.

mod expr_reads;
mod oop;
mod scope_usage;
mod unreachable;

use crate::errors::CompileWarning;
use crate::parser::ast::Program;

use oop::collect_oop_warnings;
use scope_usage::collect_function_like_warnings;
use unreachable::collect_unreachable_recursive;

/// Emits the `collect_warnings` routine.
pub fn collect_warnings(program: &Program) -> Vec<CompileWarning> {
    let mut warnings = Vec::new();
    collect_oop_warnings(program, &mut warnings);
    collect_unreachable_recursive(program, &mut warnings);
    collect_function_like_warnings(program, &mut warnings);
    warnings
}
