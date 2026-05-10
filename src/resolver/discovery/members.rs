use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::parser::ast::{ClassMethod, ClassProperty, Expr};

use super::branches::discover_isolated;
use super::exprs::discover_expr;
use super::output::DiscoveryOutput;
use super::super::state::ResolveState;

pub(super) fn discover_params(
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    for (_, _, default, _) in params {
        if let Some(default) = default {
            discover_expr(default, base_dir, loaded_paths, include_chain, state, output)?;
        }
    }
    Ok(())
}

pub(super) fn discover_properties(
    properties: &[ClassProperty],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    for property in properties {
        if let Some(default) = &property.default {
            discover_expr(default, base_dir, loaded_paths, include_chain, state, output)?;
        }
    }
    Ok(())
}

pub(super) fn discover_methods(
    methods: &[ClassMethod],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    for method in methods {
        let mut local = state.clone();
        discover_params(
            &method.params,
            base_dir,
            loaded_paths,
            include_chain,
            &mut local,
            output,
        )?;
        discover_isolated(
            &method.body,
            base_dir,
            loaded_paths,
            include_chain,
            state,
            output,
        )?;
    }
    Ok(())
}
