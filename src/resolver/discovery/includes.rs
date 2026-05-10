use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::parser::ast::Expr;

use super::output::{DiscoveryOutput, FunctionVariantRegistry};
use super::stmts::discover_stmts;
use super::super::declarations::extract_discoverable_declarations;
use super::super::engine::resolve_stmts;
use super::super::files::{parse_file, resolve_path};
use super::super::include_path::fold_include_path;
use super::super::state::ResolveState;

pub(super) fn discover_include(
    path: &Expr,
    once: bool,
    required: bool,
    span: crate::span::Span,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    let path_str = fold_include_path(path, state).map_err(|msg| CompileError::new(span, &msg))?;
    let resolved = resolve_path(&path_str, base_dir);
    let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

    if !resolved.exists() {
        if required {
            return Err(CompileError::new(
                span,
                &format!("Required file not found: '{}'", path_str),
            ));
        }
        return Ok(());
    }

    if once && loaded_paths.contains(&canonical) {
        return Ok(());
    }

    if include_chain.contains(&canonical) {
        if once {
            return Ok(());
        }
        return Err(CompileError::new(
            span,
            &format!("Circular include detected: '{}'", path_str),
        ));
    }

    let included_stmts = parse_file(&resolved, span)?;
    let included_stmts =
        crate::magic_constants::substitute_file_and_scope_constants(included_stmts, &resolved);

    let included_dir = resolved.parent().unwrap_or(base_dir);
    let mut declaration_state = state.clone();
    declaration_state.namespace = None;
    declaration_state.const_imports = HashMap::new();
    include_chain.push(canonical.clone());

    let saved_namespace = state.namespace.clone();
    let saved_imports = state.const_imports.clone();
    state.namespace = None;
    state.const_imports = HashMap::new();
    let mut nested_output = DiscoveryOutput::default();
    discover_stmts(
        &included_stmts,
        included_dir,
        loaded_paths,
        include_chain,
        state,
        &mut nested_output,
    )?;
    state.namespace = saved_namespace;
    state.const_imports = saved_imports;

    let entry_declaration_state = declaration_state.clone();
    let entry_include_chain = include_chain.clone();
    let mut declaration_declared_once = HashSet::new();
    let mut declaration_include_chain = entry_include_chain.clone();
    let mut declaration_state_for_resolution = declaration_state.clone();
    let declaration_function_variants = FunctionVariantRegistry::default();
    let resolved_declarations = resolve_stmts(
        included_stmts.clone(),
        included_dir,
        &mut declaration_declared_once,
        &mut declaration_include_chain,
        &mut declaration_state_for_resolution,
        &declaration_function_variants,
    )?;

    include_chain.pop();
    loaded_paths.insert(canonical.clone());
    if once {
        output.extend_once_guarded(nested_output);
    } else {
        output.extend(nested_output);
    }

    let file_declarations = extract_discoverable_declarations(&resolved_declarations);
    output.push(
        canonical,
        span,
        file_declarations,
        included_stmts,
        included_dir.to_path_buf(),
        entry_declaration_state,
        entry_include_chain,
        !once,
    );

    Ok(())
}
