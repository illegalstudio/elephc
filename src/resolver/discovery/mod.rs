//! Purpose:
//! Coordinates discovery of declarations made reachable by include and require statements.
//! Produces include metadata and function-variant registries before resolver expansion.
//!
//! Called from:
//! - `crate::resolver::resolve()`.
//!
//! Key details:
//! - Discovery mirrors resolver traversal enough to expose declarations without executing inactive branches twice.

mod branches;
mod exprs;
mod includes;
mod members;
mod output;
mod stmts;

use std::collections::HashSet;
use std::path::Path;

use crate::errors::CompileError;
use crate::parser::ast::Stmt;

use super::state::ResolveState;
use output::{DiscoveryOutput, IncludeDiscovery};
use stmts::discover_stmts;

pub(in crate::resolver) use output::{
    DiscoveryEntry, FunctionVariantInfo, FunctionVariantKey, FunctionVariantRegistry,
};

/// Discovers declarations reachable through include/require statements in the given AST.
///
/// Traverses all top-level statements, following include/require chains while tracking
/// loaded paths to avoid cycles. Populates `IncludeDiscovery` with the complete set of
/// reachable declarations and their source locations.
///
/// # Arguments
/// * `stmts` - Top-level statements to scan for include/require directives
/// * `base_dir` - Directory from which relative paths are resolved
///
/// # Returns
/// * `Ok(IncludeDiscovery)` with all reachable declarations grouped by source file
/// * `Err(CompileError)` if an include path is invalid, cycles are detected, or resolution fails
pub(super) fn discover_include_declarations(
    stmts: &[Stmt],
    base_dir: &Path,
) -> Result<IncludeDiscovery, CompileError> {
    let mut output = DiscoveryOutput::default();
    let mut loaded_paths = HashSet::new();
    let mut include_chain = Vec::new();
    let mut state = ResolveState::default();

    discover_stmts(
        stmts,
        base_dir,
        &mut loaded_paths,
        &mut include_chain,
        &mut state,
        &mut output,
    )?;

    output.into_include_discovery()
}
