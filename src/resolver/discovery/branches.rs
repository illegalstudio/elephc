use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Stmt};

use super::exprs::discover_expr;
use super::output::DiscoveryOutput;
use super::stmts::discover_stmts;
use super::super::state::ResolveState;

pub(super) struct BranchDiscovery {
    output: DiscoveryOutput,
    loaded_paths: HashSet<PathBuf>,
}

impl BranchDiscovery {
    fn empty(loaded_paths: &HashSet<PathBuf>) -> Self {
        Self {
            output: DiscoveryOutput::default(),
            loaded_paths: loaded_paths.clone(),
        }
    }
}

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
