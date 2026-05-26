//! Purpose:
//! Discovers include effects inside class properties, methods, and parameter defaults.
//! Bridges declaration-member AST nodes into expression and branch discovery.
//!
//! Called from:
//! - `crate::resolver::discovery::stmts`.
//!
//! Key details:
//! - Member discovery must not lose declarations reachable from default expressions or method bodies.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::parser::ast::{ClassMethod, ClassProperty, Expr};

use super::branches::discover_isolated;
use super::exprs::discover_expr;
use super::output::DiscoveryOutput;
use super::super::state::ResolveState;

/// Discovers include effects in function or method parameter defaults.
///
/// Iterates over each parameter and, if it has a default expression, passes it to
/// `discover_expr` to scan for `include`/`require` effects. The state is shared
/// across all parameters — declarations found in one default are visible to
/// subsequent defaults and to the caller.
///
/// # Inputs
/// - `params`: slice of `(name, type, default, is_ref)` parameter tuples
/// - `base_dir`: directory used to resolve relative include paths
/// - `loaded_paths`: set of already-loaded paths (accumulated across discovery)
/// - `include_chain`: current include chain for cycle detection
/// - `state`: shared resolver state (mutated in place)
/// - `output`: discovery output accumulator (mutated in place)
///
/// # Returns
/// `Ok(())` on success; first `CompileError` aborts discovery.
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

/// Discovers include effects in class property defaults.
///
/// Iterates over each `ClassProperty` and, if it has a default expression, passes
/// it to `discover_expr`. Declarations found are accumulated into `state` and
/// `output` for downstream use.
///
/// # Inputs
/// - `properties`: slice of class property AST nodes
/// - `base_dir`: directory used to resolve relative include paths
/// - `loaded_paths`: set of already-loaded paths (accumulated across discovery)
/// - `include_chain`: current include chain for cycle detection
/// - `state`: shared resolver state (mutated in place)
/// - `output`: discovery output accumulator (mutated in place)
///
/// # Returns
/// `Ok(())` on success; first `CompileError` aborts discovery.
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

/// Discovers include effects in class method bodies and their parameter defaults.
///
/// Clones the resolver state for each method to create an isolated scope, then
/// runs `discover_params` on the method's parameters and `discover_isolated` on
/// the method body. The cloned state is discarded after each method — effects
/// found inside a method body are NOT propagated back to the caller, preserving
/// lexical isolation.
///
/// # Inputs
/// - `methods`: slice of class method AST nodes
/// - `base_dir`: directory used to resolve relative include paths
/// - `loaded_paths`: set of already-loaded paths (accumulated across discovery)
/// - `include_chain`: current include chain for cycle detection
/// - `state`: original resolver state (used as base for cloning; not mutated)
/// - `output`: discovery output accumulator (mutated in place)
///
/// # Returns
/// `Ok(())` on success; first `CompileError` aborts discovery.
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
