//! Purpose:
//! Walks statements to discover include-reachable declarations before full resolver expansion.
//! Handles namespace context, constant definitions, includes, branches, and class-like members.
//!
//! Called from:
//! - `crate::resolver::discovery::discover_include_declarations()`.
//!
//! Key details:
//! - Discovery follows PHP statement order so constants and namespace context affect later include paths.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::names::canonical_name_for_decl;
use crate::parser::ast::{CatchClause, ExprKind, Stmt, StmtKind};

use super::branches::{
    constant_truthiness, discover_branch_output, discover_if_tail, discover_isolated,
    discover_isolated_output, exclusive_group_id,
};
use super::exprs::discover_expr;
use super::includes::discover_include;
use super::members::{discover_methods, discover_params, discover_properties};
use super::output::DiscoveryOutput;
use super::super::include_path::fold_include_path;
use super::super::state::{
    is_define_call_name, namespace_string, normalize_defined_constant_name,
    register_const_imports, ResolveState,
};

pub(super) fn discover_stmts(
    stmts: &[Stmt],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    for stmt in stmts {
        discover_stmt(stmt, base_dir, loaded_paths, include_chain, state, output)?;
    }
    Ok(())
}

fn discover_stmt(
    stmt: &Stmt,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    match &stmt.kind {
        StmtKind::Include { path, once, required } => {
            discover_include(
                path,
                *once,
                *required,
                stmt.span,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        StmtKind::ConstDecl { name, value } => {
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            if let Ok(s) = fold_include_path(value, state) {
                let key = canonical_name_for_decl(state.namespace.as_deref(), name);
                state.constants.insert(key, s);
            }
        }
        StmtKind::ExprStmt(expr) => {
            discover_expr(expr, base_dir, loaded_paths, include_chain, state, output)?;
            if let ExprKind::FunctionCall { name, args } = &expr.kind {
                if is_define_call_name(name) && args.len() == 2 {
                    if let ExprKind::StringLiteral(const_name) = &args[0].kind {
                        if let Ok(value) = fold_include_path(&args[1], state) {
                            state
                                .constants
                                .insert(normalize_defined_constant_name(const_name), value);
                        }
                    }
                }
            }
        }
        StmtKind::NamespaceDecl { name } => {
            state.namespace = Some(namespace_string(name));
            state.const_imports = HashMap::new();
        }
        StmtKind::NamespaceBlock { name, body } => {
            let saved_namespace = state.namespace.clone();
            let saved_imports = state.const_imports.clone();
            state.namespace = Some(namespace_string(name));
            state.const_imports = HashMap::new();
            discover_stmts(body, base_dir, loaded_paths, include_chain, state, output)?;
            state.namespace = saved_namespace;
            state.const_imports = saved_imports;
        }
        StmtKind::UseDecl { .. } => {
            register_const_imports(state, stmt);
        }
        StmtKind::Synthetic(body) | StmtKind::IncludeOnceGuard { body, .. } => {
            discover_isolated(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            discover_expr(condition, base_dir, loaded_paths, include_chain, state, output)?;
            let group_id = exclusive_group_id(stmt.span, base_dir, include_chain);

            match constant_truthiness(condition) {
                Some(true) => {
                    discover_isolated(
                        then_body,
                        base_dir,
                        loaded_paths,
                        include_chain,
                        state,
                        output,
                    )?;
                }
                Some(false) => discover_if_tail(
                    elseif_clauses,
                    else_body.as_deref(),
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                    group_id,
                    Vec::new(),
                )?,
                None => {
                    let then_output = discover_branch_output(
                        then_body,
                        base_dir,
                        loaded_paths,
                        include_chain,
                        state,
                    )?;
                    discover_if_tail(
                        elseif_clauses,
                        else_body.as_deref(),
                        base_dir,
                        loaded_paths,
                        include_chain,
                        state,
                        output,
                        group_id,
                        vec![then_output],
                    )?;
                }
            }
        }
        StmtKind::While { condition, body } => {
            discover_expr(condition, base_dir, loaded_paths, include_chain, state, output)?;
            if constant_truthiness(condition) != Some(false) {
                let body_output =
                    discover_isolated_output(body, base_dir, loaded_paths, include_chain, state)?;
                output.extend_loop_body(body_output);
            }
        }
        StmtKind::DoWhile { condition, body } => {
            let body_output =
                discover_isolated_output(body, base_dir, loaded_paths, include_chain, state)?;
            discover_expr(condition, base_dir, loaded_paths, include_chain, state, output)?;
            if constant_truthiness(condition) == Some(false) {
                output.extend(body_output);
            } else {
                output.extend_loop_body(body_output);
            }
        }
        StmtKind::For { init, condition, update, body } => {
            if let Some(init) = init {
                discover_stmt(init, base_dir, loaded_paths, include_chain, state, output)?;
            }
            if let Some(condition) = condition {
                discover_expr(condition, base_dir, loaded_paths, include_chain, state, output)?;
            }
            if condition.as_ref().and_then(constant_truthiness) != Some(false) {
                let mut loop_output =
                    discover_isolated_output(body, base_dir, loaded_paths, include_chain, state)?;
                if let Some(update) = update {
                    let mut update_state = state.clone();
                    discover_stmt(
                        update,
                        base_dir,
                        loaded_paths,
                        include_chain,
                        &mut update_state,
                        &mut loop_output,
                    )?;
                }
                output.extend_loop_body(loop_output);
            }
        }
        StmtKind::Foreach { array, body, .. } => {
            discover_expr(array, base_dir, loaded_paths, include_chain, state, output)?;
            let body_output =
                discover_isolated_output(body, base_dir, loaded_paths, include_chain, state)?;
            output.extend_loop_body(body_output);
        }
        StmtKind::Switch { subject, cases, default } => {
            discover_expr(subject, base_dir, loaded_paths, include_chain, state, output)?;
            for (values, body) in cases {
                for value in values {
                    discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
                }
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                )?;
            }
            if let Some(body) = default {
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                )?;
            }
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            discover_isolated(
                try_body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
            for CatchClause { body, .. } in catches {
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                )?;
            }
            if let Some(body) = finally_body {
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                )?;
            }
        }
        StmtKind::FunctionDecl { params, body, .. } => {
            discover_params(params, base_dir, loaded_paths, include_chain, state, output)?;
            discover_isolated(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        StmtKind::ClassDecl { properties, methods, .. }
        | StmtKind::TraitDecl { properties, methods, .. } => {
            discover_properties(
                properties,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
            discover_methods(
                methods,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        StmtKind::InterfaceDecl { methods, .. } => {
            discover_methods(
                methods,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        StmtKind::EnumDecl { cases, .. } => {
            for case in cases {
                if let Some(value) = &case.value {
                    discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
                }
            }
        }
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::Return(Some(expr))
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. } => {
            discover_expr(expr, base_dir, loaded_paths, include_chain, state, output)?;
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            discover_expr(index, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
        }
        StmtKind::NestedArrayAssign { target, value } => {
            discover_expr(target, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
        }
        StmtKind::Return(None)
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::IfDef { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
    }

    Ok(())
}
