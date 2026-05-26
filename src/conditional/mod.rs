//! Purpose:
//! Applies compiler `ifdef` conditionals to remove inactive AST branches.
//! Coordinates statement and expression rewrites against symbols defined by the CLI.
//!
//! Called from:
//! - `crate::pipeline::compile()` after magic constants and before include resolution.
//!
//! Key details:
//! - Inactive branches are removed before resolver/type-checker stages can observe their declarations or errors.

mod exprs;
mod stmts;

use std::collections::HashSet;

use crate::parser::ast::Program;

/// Removes inactive `ifdef` branches from the program based on CLI-defined symbols.
pub fn apply(program: Program, defines: &HashSet<String>) -> Program {
    stmts::apply_stmts(program, defines)
}
