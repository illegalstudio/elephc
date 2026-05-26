//! Purpose:
//! Discovers include declarations across conditional and branch-like AST regions.
//! Merges isolated branch outputs while tracking mutually exclusive declaration groups.
//!
//! Called from:
//! - `crate::resolver::discovery::stmts` and expression discovery helpers.
//!
//! Key details:
//! - Constant branch truthiness is used only when it can be determined statically and conservatively.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Stmt};

use super::exprs::discover_expr;
use super::output::DiscoveryOutput;
use super::stmts::discover_stmts;
use super::super::state::ResolveState;

/// Holds the result of discovering declarations within a single branch region.
/// Tracks the output declarations and the set of paths loaded within that branch.
pub(super) struct BranchDiscovery {
    output: DiscoveryOutput,
    loaded_paths: HashSet<PathBuf>,
}

impl BranchDiscovery {
    /// Creates a `BranchDiscovery` with no declarations, preserving the loaded paths set.
    fn empty(loaded_paths: &HashSet<PathBuf>) -> Self {
        Self {
            output: DiscoveryOutput::default(),
            loaded_paths: loaded_paths.clone(),
        }
    }
}

/// Discovers declarations in an isolated statement block, returning a fresh `DiscoveryOutput`.
/// The caller receives all discovered declarations without modifying the shared output.
pub(super) fn discover_isolated_output(
    stmts: &[Stmt],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<DiscoveryOutput, CompileError> {
    let mut local = state.clone();
    let mut output = DiscoveryOutput::default();
    discover_stmts(
        stmts,
        base_dir,
        loaded_paths,
        include_chain,
        &mut local,
        &mut output,
    )?;
    Ok(output)
}

/// Discovers declarations in an isolated statement block, merging into `output` in place.
pub(super) fn discover_isolated(
    stmts: &[Stmt],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    output.extend(discover_isolated_output(
        stmts,
        base_dir,
        loaded_paths,
        include_chain,
        state,
    )?);
    Ok(())
}

/// Discovers declarations in a branch region, returning a `BranchDiscovery` that holds
/// local copies of the state, loaded paths, and include chain specific to that branch.
pub(super) fn discover_branch_output(
    stmts: &[Stmt],
    base_dir: &Path,
    loaded_paths: &HashSet<PathBuf>,
    include_chain: &[PathBuf],
    state: &ResolveState,
) -> Result<BranchDiscovery, CompileError> {
    let mut local_state = state.clone();
    let mut local_loaded_paths = loaded_paths.clone();
    let mut local_include_chain = include_chain.to_vec();
    let mut output = DiscoveryOutput::default();
    discover_stmts(
        stmts,
        base_dir,
        &mut local_loaded_paths,
        &mut local_include_chain,
        &mut local_state,
        &mut output,
    )?;
    Ok(BranchDiscovery {
        output,
        loaded_paths: local_loaded_paths,
    })
}

/// Merges multiple branch discoveries into a single `DiscoveryOutput`.
/// Loaded paths are intersection-based: only paths present in every branch survive.
/// Alternatives are marked with `group_id` to track mutually exclusive declaration groups.
fn merge_branch_discoveries(
    branches: Vec<BranchDiscovery>,
    loaded_paths: &mut HashSet<PathBuf>,
    group_id: String,
) -> DiscoveryOutput {
    let mut outputs = Vec::with_capacity(branches.len());
    let mut merged_loaded_paths: Option<HashSet<PathBuf>> = None;

    for branch in branches {
        match &mut merged_loaded_paths {
            Some(paths) => {
                paths.retain(|path| branch.loaded_paths.contains(path));
            }
            None => {
                merged_loaded_paths = Some(branch.loaded_paths);
            }
        }
        outputs.push(branch.output);
    }

    if let Some(paths) = merged_loaded_paths {
        *loaded_paths = paths;
    }

    DiscoveryOutput::merge_alternatives(outputs, group_id)
}

#[allow(clippy::too_many_arguments)]
/// Processes if/elseif/else chains for declaration discovery.
/// For each elseif clause, evaluates `constant_truthiness` to determine if the branch
/// is statically reachable. If the condition is known true, explores only that branch
/// and returns. If known false, skips it. If unknown, accumulates it as a potential branch.
/// Finally, appends the else body (or an empty branch if no else) and merges all alternatives.
/// Updates `loaded_paths` to retain only paths loaded in every branch explored so far.
pub(super) fn discover_if_tail(
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    output: &mut DiscoveryOutput,
    group_id: String,
    mut alternatives: Vec<BranchDiscovery>,
) -> Result<(), CompileError> {
    for (condition, body) in elseif_clauses {
        let mut condition_state = state.clone();
        discover_expr(
            condition,
            base_dir,
            loaded_paths,
            include_chain,
            &mut condition_state,
            output,
        )?;

        match constant_truthiness(condition) {
            Some(false) => {}
            Some(true) => {
                alternatives.push(discover_branch_output(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                )?);
                output.extend(merge_branch_discoveries(
                    alternatives,
                    loaded_paths,
                    group_id,
                ));
                return Ok(());
            }
            None => alternatives.push(discover_branch_output(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
            )?),
        }
    }

    alternatives.push(match else_body {
        Some(body) => discover_branch_output(body, base_dir, loaded_paths, include_chain, state)?,
        None => BranchDiscovery::empty(loaded_paths),
    });
    output.extend(merge_branch_discoveries(
        alternatives,
        loaded_paths,
        group_id,
    ));
    Ok(())
}

/// Generates a unique group identifier for mutually exclusive declaration branches.
/// Format: `"{owner_path}:{line}:{col}"` where `owner` is the last path in `include_chain`,
/// or `base_dir` if the chain is empty. Used to correlate branches that cannot both be active.
pub(super) fn exclusive_group_id(
    span: crate::span::Span,
    base_dir: &Path,
    include_chain: &[PathBuf],
) -> String {
    let owner = include_chain
        .last()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| base_dir.to_string_lossy().into_owned());
    format!("{}:{}:{}", owner, span.line, span.col)
}

/// Evaluates a constant expression's truthiness statically.
/// Returns `Some(true)` if the expression is a truthy literal, `Some(false)` if falsy,
/// or `None` if the truthiness cannot be determined conservatively (e.g., variables,
/// function calls, or runtime-dependent expressions).
pub(super) fn constant_truthiness(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::Null => Some(false),
        ExprKind::IntLiteral(value) => Some(*value != 0),
        ExprKind::FloatLiteral(value) => Some(*value != 0.0),
        ExprKind::StringLiteral(value) => Some(!(value.is_empty() || value == "0")),
        _ => None,
    }
}
