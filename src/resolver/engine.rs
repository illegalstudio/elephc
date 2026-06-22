//! Purpose:
//! Resolves statement lists by expanding include/require effects into the AST.
//! Tracks included files, declaration state, constants, and function variants across traversal.
//!
//! Called from:
//! - `crate::resolver::resolve()` and isolated include-discovery resolution.
//!
//! Key details:
//! - Resolver runs before namespace name canonicalization, so it preserves PHP namespace syntax context.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::names::canonical_name_for_decl;
use crate::parser::ast::{CatchClause, ClassMethod, ExprKind, Stmt, StmtKind};

use super::discovery::FunctionVariantRegistry;
use super::engine_includes::{expand_value_include, resolve_include_stmt, IncludeValueCapture};
use super::include_path::fold_include_path;
use super::state::{
    is_define_call_name, namespace_string, normalize_defined_constant_name,
    register_const_imports, ResolveState,
};
use super::stmt_exprs::resolve_stmt_exprs;

/// Resolves a list of statements, expanding include/require effects and tracking
/// constants, namespaces, and function variants.
///
/// For control-flow statements (If, While, For, etc.), the body is resolved in
/// isolation so include/constant state does not leak across branches. Class and
/// interface method bodies are likewise resolved in isolation. Include statements
/// are expanded inline; const declarations and `define()` calls populate `state.constants`.
/// Namespace and use declarations update `state` for subsequent statements.
/// If `stmt` is an expression-position include (`$x = require X;` or `return require X;`),
/// expands it into a sequence of caller-scope statements and returns them; otherwise returns
/// `Ok(None)` so the statement is resolved normally.
fn try_expand_value_include(
    stmt: &Stmt,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<Option<Vec<Stmt>>, CompileError> {
    let value = match &stmt.kind {
        StmtKind::Assign { value, .. } => value,
        StmtKind::Return(Some(value)) => value,
        _ => return Ok(None),
    };
    let ExprKind::IncludeValue {
        path,
        once,
        required,
    } = &value.kind
    else {
        return Ok(None);
    };
    let capture = match &stmt.kind {
        StmtKind::Assign { name, .. } => IncludeValueCapture::Assign(name.clone()),
        _ => IncludeValueCapture::Return,
    };
    let expanded = expand_value_include(
        stmt.span,
        path,
        *once,
        *required,
        capture,
        base_dir,
        declared_once,
        include_chain,
        state,
        function_variants,
    )?;
    Ok(Some(expanded))
}

/// Resolves a statement list by expanding includes, resolving expression-position
/// includes, and preserving include-once/function-variant bookkeeping.
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
        // Expression-position includes (`$x = require X;` / `return require X;`) are expanded
        // before generic expression resolution so the included file's statements are inlined into
        // the caller's scope rather than resolved as an opaque sub-expression.
        if let Some(expanded) = try_expand_value_include(
            &stmt,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )? {
            result.extend(expanded);
            continue;
        }

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
                if let Some(resolved) = resolve_include_stmt(
                    &stmt,
                    path,
                    *once,
                    *required,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )? {
                    result.extend(resolved);
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
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                value_by_ref,
                body,
            } => {
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
                        value_by_ref: *value_by_ref,
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
            StmtKind::FunctionDecl { name, params, variadic, variadic_type, return_type, body } => {
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
                        variadic_type: variadic_type.clone(),
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
            constants,
            } => {
                let methods = resolve_methods(
                    methods,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                result.push(Stmt::with_attributes(
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
                        constants: constants.clone(),
                    },
                    stmt.span,
                    stmt.attributes.clone(),
                ));
            }
            StmtKind::InterfaceDecl { name, extends, properties, methods,
            constants,
            } => {
                let methods = resolve_methods(
                    methods,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                result.push(Stmt::with_attributes(
                    StmtKind::InterfaceDecl {
                        name: name.clone(),
                        extends: extends.clone(),
                        properties: properties.clone(),
                        methods,
                        constants: constants.clone(),
                    },
                    stmt.span,
                    stmt.attributes.clone(),
                ));
            }
            StmtKind::TraitDecl {
                name,
                trait_uses,
                properties,
                methods,
            constants,
            } => {
                let methods = resolve_methods(
                    methods,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?;
                result.push(Stmt::with_attributes(
                    StmtKind::TraitDecl {
                        name: name.clone(),
                        trait_uses: trait_uses.clone(),
                        properties: properties.clone(),
                        methods,
                        constants: constants.clone(),
                    },
                    stmt.span,
                    stmt.attributes.clone(),
                ));
            }
            _ => {
                result.push(stmt);
            }
        }
    }

    Ok(result)
}

/// Resolves statements in an isolated scope by cloning `state` so that include
/// and constant effects do not leak back to the caller.
///
/// Used for function bodies, method bodies, and control-structure branches
/// (If/While/For/etc.) where each branch should see a clean view of constants
/// and includes accumulated up to that point.
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

/// Resolves the body of each class method in isolation, cloning `state` for each
/// method so that include/constant effects inside one method do not leak to others.
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
