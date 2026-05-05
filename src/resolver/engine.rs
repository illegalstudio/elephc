use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::names::canonical_name_for_decl;
use crate::parser::ast::{CatchClause, ClassMethod, ExprKind, Stmt, StmtKind};

use super::declarations::strip_discoverable_declarations;
use super::discovery::FunctionVariantRegistry;
use super::files::{parse_file, resolve_path};
use super::include_once::include_once_label;
use super::include_path::fold_include_path;
use super::state::{
    is_define_call_name, namespace_string, normalize_defined_constant_name,
    register_const_imports, ResolveState,
};
use super::stmt_exprs::resolve_stmt_exprs;

pub(super) fn resolve_stmts(
    stmts: Vec<Stmt>,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<Vec<Stmt>, CompileError> {
    let mut result = Vec::new();

    for stmt in stmts {
        let stmt = resolve_stmt_exprs(
            stmt,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?;
        match &stmt.kind {
            StmtKind::Include { path, once, required } => {
                let path_str = fold_include_path(path, state)
                    .map_err(|msg| CompileError::new(stmt.span, &msg))?;
                let resolved = resolve_path(&path_str, base_dir);
                let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

                if !resolved.exists() {
                    if *required {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!("Required file not found: '{}'", path_str),
                        ));
                    }
                    continue;
                }

                if include_chain.contains(&canonical) {
                    if *once {
                        continue;
                    }
                    return Err(CompileError::new(
                        stmt.span,
                        &format!("Circular include detected: '{}'", path_str),
                    ));
                }

                let included_stmts = parse_file(&resolved, stmt.span)?;
                let included_stmts =
                    crate::magic_constants::substitute_file_and_scope_constants(included_stmts, &resolved);

                let included_dir = resolved.parent().unwrap_or(base_dir);
                include_chain.push(canonical.clone());

                let saved_namespace = state.namespace.clone();
                let saved_imports = state.const_imports.clone();
                state.namespace = None;
                state.const_imports = HashMap::new();
                let resolved_stmts = resolve_stmts(
                    included_stmts,
                    included_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                state.namespace = saved_namespace;
                state.const_imports = saved_imports;

                include_chain.pop();

                let include_label = include_once_label(&canonical);
                let executable = strip_discoverable_declarations(
                    resolved_stmts,
                    Some(&canonical),
                    function_variants,
                );
                if *once {
                    // Declaration discovery already hoisted compile-time declarations;
                    // executable include body statements are guarded so runtime order matches PHP.
                    declared_once.insert(canonical);
                    result.push(Stmt::new(
                        StmtKind::IncludeOnceGuard {
                            label: include_label,
                            body: vec![Stmt::new(
                                StmtKind::NamespaceBlock {
                                    name: None,
                                    body: executable,
                                },
                                stmt.span,
                            )],
                        },
                        stmt.span,
                    ));
                } else {
                    // Regular includes still mark the file as loaded for a later
                    // include_once/require_once, while executable statements stay at
                    // the include point.
                    declared_once.insert(canonical);
                    result.push(Stmt::new(
                        StmtKind::IncludeOnceMark {
                            label: include_label,
                        },
                        stmt.span,
                    ));
                    result.push(Stmt::new(
                        StmtKind::NamespaceBlock {
                            name: None,
                            body: executable,
                        },
                        stmt.span,
                    ));
                }
            }
            StmtKind::ConstDecl { name, value } => {
                if let Ok(s) = fold_include_path(value, state) {
                    let key = canonical_name_for_decl(state.namespace.as_deref(), name);
                    state.constants.insert(key, s);
                }
                result.push(stmt);
            }
            StmtKind::ExprStmt(expr) => {
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
                result.push(stmt);
            }
            StmtKind::NamespaceDecl { name } => {
                state.namespace = Some(namespace_string(name));
                state.const_imports = HashMap::new();
                result.push(stmt);
            }
            StmtKind::NamespaceBlock { name, body } => {
                let saved_namespace = state.namespace.clone();
                let saved_imports = state.const_imports.clone();
                state.namespace = Some(namespace_string(name));
                state.const_imports = HashMap::new();
                let body_resolved = resolve_stmts(
                    body.clone(),
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                state.namespace = saved_namespace;
                state.const_imports = saved_imports;
                result.push(Stmt::new(
                    StmtKind::NamespaceBlock {
                        name: name.clone(),
                        body: body_resolved,
                    },
                    stmt.span,
                ));
            }
            StmtKind::UseDecl { .. } => {
                register_const_imports(state, &stmt);
                result.push(stmt);
            }
            StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
                let then_body = resolve_isolated(
                    then_body.clone(),
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                let elseif_clauses = elseif_clauses
                    .iter()
                    .map(|(cond, body)| {
                        Ok((
                            cond.clone(),
                            resolve_isolated(
                                body.clone(),
                                base_dir,
                                declared_once,
                                include_chain,
                                state,
                                function_variants,
                            )?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?;
                let else_body = else_body
                    .as_ref()
                    .map(|body| {
                        resolve_isolated(
                            body.clone(),
                            base_dir,
                            declared_once,
                            include_chain,
                            state,
                            function_variants,
                        )
                    })
                    .transpose()?;
                result.push(Stmt::new(
                    StmtKind::If {
                        condition: condition.clone(),
                        then_body,
                        elseif_clauses,
                        else_body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::While { condition, body } => {
                let body = resolve_isolated(
                    body.clone(),
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                result.push(Stmt::new(
                    StmtKind::While {
                        condition: condition.clone(),
                        body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::DoWhile { body, condition } => {
                let body = resolve_isolated(
                    body.clone(),
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                result.push(Stmt::new(
                    StmtKind::DoWhile {
                        body,
                        condition: condition.clone(),
                    },
                    stmt.span,
                ));
            }
            StmtKind::For { init, condition, update, body } => {
                let body = resolve_isolated(
                    body.clone(),
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                result.push(Stmt::new(
                    StmtKind::For {
                        init: init.clone(),
                        condition: condition.clone(),
                        update: update.clone(),
                        body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::Foreach { array, key_var, value_var, body } => {
                let body = resolve_isolated(
                    body.clone(),
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                result.push(Stmt::new(
                    StmtKind::Foreach {
                        array: array.clone(),
                        key_var: key_var.clone(),
                        value_var: value_var.clone(),
                        body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::Switch { subject, cases, default } => {
                let cases = cases
                    .iter()
                    .map(|(values, body)| {
                        Ok((
                            values.clone(),
                            resolve_isolated(
                                body.clone(),
                                base_dir,
                                declared_once,
                                include_chain,
                                state,
                                function_variants,
                            )?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?;
                let default = default
                    .as_ref()
                    .map(|body| {
                        resolve_isolated(
                            body.clone(),
                            base_dir,
                            declared_once,
                            include_chain,
                            state,
                            function_variants,
                        )
                    })
                    .transpose()?;
                result.push(Stmt::new(
                    StmtKind::Switch {
                        subject: subject.clone(),
                        cases,
                        default,
                    },
                    stmt.span,
                ));
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                let try_body = resolve_isolated(
                    try_body.clone(),
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                let catches = catches
                    .iter()
                    .map(|catch_clause| {
                        Ok(CatchClause {
                            exception_types: catch_clause.exception_types.clone(),
                            variable: catch_clause.variable.clone(),
                            body: resolve_isolated(
                                catch_clause.body.clone(),
                                base_dir,
                                declared_once,
                                include_chain,
                                state,
                                function_variants,
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?;
                let finally_body = finally_body
                    .as_ref()
                    .map(|body| {
                        resolve_isolated(
                            body.clone(),
                            base_dir,
                            declared_once,
                            include_chain,
                            state,
                            function_variants,
                        )
                    })
                    .transpose()?;
                result.push(Stmt::new(
                    StmtKind::Try {
                        try_body,
                        catches,
                        finally_body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::FunctionDecl { name, params, variadic, return_type, body } => {
                let body = resolve_isolated(
                    body.clone(),
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                result.push(Stmt::new(
                    StmtKind::FunctionDecl {
                        name: name.clone(),
                        params: params.clone(),
                        variadic: variadic.clone(),
                        return_type: return_type.clone(),
                        body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::ClassDecl {
                name,
                extends,
                implements,
                is_abstract,
                is_final,
                is_readonly_class,
                trait_uses,
                properties,
                methods,
            } => {
                let methods = resolve_methods(
                    methods,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                result.push(Stmt::new(
                    StmtKind::ClassDecl {
                        name: name.clone(),
                        extends: extends.clone(),
                        implements: implements.clone(),
                        is_abstract: *is_abstract,
                        is_final: *is_final,
                        is_readonly_class: *is_readonly_class,
                        trait_uses: trait_uses.clone(),
                        properties: properties.clone(),
                        methods,
                    },
                    stmt.span,
                ));
            }
            StmtKind::InterfaceDecl { name, extends, methods } => {
                let methods = resolve_methods(
                    methods,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                result.push(Stmt::new(
                    StmtKind::InterfaceDecl {
                        name: name.clone(),
                        extends: extends.clone(),
                        methods,
                    },
                    stmt.span,
                ));
            }
            StmtKind::TraitDecl {
                name,
                trait_uses,
                properties,
                methods,
            } => {
                let methods = resolve_methods(
                    methods,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                result.push(Stmt::new(
                    StmtKind::TraitDecl {
                        name: name.clone(),
                        trait_uses: trait_uses.clone(),
                        properties: properties.clone(),
                        methods,
                    },
                    stmt.span,
                ));
            }
            _ => {
                result.push(stmt);
            }
        }
    }

    Ok(result)
}

pub(super) fn resolve_isolated(
    stmts: Vec<Stmt>,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<Vec<Stmt>, CompileError> {
    let mut local = state.clone();
    resolve_stmts(
        stmts,
        base_dir,
        declared_once,
        include_chain,
        &mut local,
        function_variants,
    )
}

fn resolve_methods(
    methods: &[ClassMethod],
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<Vec<ClassMethod>, CompileError> {
    methods
        .iter()
        .map(|method| {
            let body = resolve_isolated(
                method.body.clone(),
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?;
            Ok(ClassMethod {
                body,
                ..method.clone()
            })
        })
        .collect()
}
