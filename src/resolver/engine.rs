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
use crate::names::{canonical_name_for_decl, Name, NameKind, DYNAMIC_INCLUDE_FUNCTION};
use crate::parser::ast::{BinOp, CatchClause, ClassMethod, Expr, ExprKind, Stmt, StmtKind};

use super::discovery::FunctionVariantRegistry;
use super::engine_includes::{
    discard_first_include_return, expand_value_include, resolve_include_stmt,
    IncludeValueCapture,
};
use super::include_path::{
    fold_include_path, is_runtime_dynamic_include_path, stays_runtime_include,
};
use super::state::{
    is_define_call_name, namespace_string, normalize_defined_constant_name,
    register_const_imports, ResolveState,
};

/// Builds the internal runtime include call used when an include must remain expression-valued.
pub(super) fn dynamic_include_call_kind(
    path: Expr,
    once: bool,
    required: bool,
    span: crate::span::Span,
) -> ExprKind {
    ExprKind::FunctionCall {
        name: Name::from_parts(
            NameKind::FullyQualified,
            vec![DYNAMIC_INCLUDE_FUNCTION.to_string()],
        ),
        args: vec![
            path,
            Expr::new(ExprKind::BoolLiteral(once), span),
            Expr::new(ExprKind::BoolLiteral(required), span),
        ],
    }
}
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
    if stays_runtime_include(path, state) {
        let value = Expr::new(
            dynamic_include_call_kind((**path).clone(), *once, *required, stmt.span),
            stmt.span,
        );
        let rewritten = match capture {
            IncludeValueCapture::Assign(name) => {
                Stmt::new(StmtKind::Assign { name, value }, stmt.span)
            }
            IncludeValueCapture::Return => Stmt::new(StmtKind::Return(Some(value)), stmt.span),
        };
        return Ok(Some(vec![rewritten]));
    }
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

/// Splits a statically resolvable include out of the statement that evaluates it FIRST.
///
/// `try_expand_value_include` compiles an include written as `$x = require X;` or
/// `return require X;`; in any other position the include stayed a runtime call and the file ran
/// in the interpreter on every execution. Composer's generated `autoload_runtime.php` opens with
/// `if (true === (require_once __DIR__.'/autoload.php') || …)`, so the whole Composer bootstrap
/// (`autoload.php`, `ComposerAutoloaderInit…::getLoader()`) was interpreted on every Symfony
/// request: about 11% of `/`.
///
/// When nothing the statement evaluates before the include can have an effect, running the
/// include first as `$tmp = require X;` and then the statement with `$tmp` in its place is the
/// same program, and the assignment form is the one `try_expand_value_include` compiles. What
/// may be passed on the way to the include is therefore limited to literals, and the walk never
/// enters an operand that might not be evaluated (the right side of `&&`, `||`, `??`), nor a
/// loop condition, nor an `elseif`, which only run conditionally or repeatedly.
fn hoist_leading_value_include(stmt: &Stmt) -> Option<Vec<Stmt>> {
    let mut rewritten = stmt.clone();
    let tmp = format!(
        "__elephc_inc_hoist_{}",
        HOISTED_INCLUDE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let include = match &mut rewritten.kind {
        StmtKind::If { condition, .. } => take_leading_include(condition, &tmp)?,
        StmtKind::ExprStmt(expr) | StmtKind::Echo(expr) => take_leading_include(expr, &tmp)?,
        // A direct `$x = require X;` / `return require X;` is already compiled as it stands.
        StmtKind::Assign { value, .. } | StmtKind::Return(Some(value))
            if !matches!(value.kind, ExprKind::IncludeValue { .. }) =>
        {
            take_leading_include(value, &tmp)?
        }
        _ => return None,
    };
    let hoisted = Stmt::new(
        StmtKind::Assign {
            name: tmp,
            value: include,
        },
        stmt.span,
    );
    Some(vec![hoisted, rewritten])
}

/// Rewrites the statement `require COND ? 'a.php' : 'b.php';` into
/// `if (COND) { require 'a.php'; } else { require 'b.php'; }` when every selectable path is
/// static, so both files join the closed world like any include under an `if`. Carbon picks its
/// period and translator base classes this way on `PHP_VERSION`; constant folding later drops
/// the branch the target never takes. Returns `None` for any other include.
fn ternary_include_as_if(stmt: &Stmt, path: &Expr, once: bool, required: bool) -> Option<Stmt> {
    let ExprKind::Ternary { condition, then_expr, else_expr } = &path.kind else {
        return None;
    };
    if !include_branch_is_static(then_expr) || !include_branch_is_static(else_expr) {
        return None;
    }
    let branch = |path: &Expr| {
        vec![Stmt::new(
            StmtKind::Include { path: path.clone(), once, required },
            stmt.span,
        )]
    };
    Some(Stmt::new(
        StmtKind::If {
            condition: (**condition).clone(),
            then_body: branch(then_expr),
            elseif_clauses: Vec::new(),
            else_body: Some(branch(else_expr)),
        },
        stmt.span,
    ))
}

/// Whether a ternary include branch selects only compile-time paths, nested ternaries included.
fn include_branch_is_static(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Ternary { then_expr, else_expr, .. } => {
            include_branch_is_static(then_expr) && include_branch_is_static(else_expr)
        }
        _ => !super::include_path::is_runtime_dynamic_include_path(expr),
    }
}

/// Numbers the temporaries `hoist_leading_value_include` introduces.
static HOISTED_INCLUDE_COUNTER: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Replaces the include `expr` evaluates first with `$tmp` and returns it, or leaves `expr`
/// untouched when an effect could precede the include (see `hoist_leading_value_include`).
fn take_leading_include(expr: &mut Expr, tmp: &str) -> Option<Expr> {
    match &mut expr.kind {
        ExprKind::IncludeValue { path, .. } if !is_runtime_dynamic_include_path(path) => {
            let variable = Expr::new(ExprKind::Variable(tmp.to_string()), expr.span);
            Some(std::mem::replace(expr, variable))
        }
        ExprKind::BinaryOp { left, op, right } => {
            if let Some(include) = take_leading_include(left, tmp) {
                return Some(include);
            }
            let right_always_runs = !matches!(op, BinOp::And | BinOp::Or | BinOp::NullCoalesce);
            if right_always_runs && is_effect_free_literal(left) {
                take_leading_include(right, tmp)
            } else {
                None
            }
        }
        ExprKind::Not(inner)
        | ExprKind::Negate(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Cast { expr: inner, .. } => take_leading_include(inner, tmp),
        _ => None,
    }
}

/// Whether evaluating `expr` can have no effect at all, so an include after it may run first.
fn is_effect_free_literal(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::BoolLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::Null
    )
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
        let _source_mode = crate::source::scoped_parse_mode(stmt.profile());
        if let Some(split) = hoist_leading_value_include(&stmt) {
            result.extend(resolve_stmts(
                split,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?);
            continue;
        }
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
                if let Some(branches) = ternary_include_as_if(&stmt, path, *once, *required) {
                    result.extend(resolve_stmts(
                        vec![branches],
                        base_dir,
                        declared_once,
                        include_chain,
                        state,
                        function_variants,
                    )?);
                    continue;
                }
                if stays_runtime_include(path, state) {
                    result.push(stmt);
                    continue;
                }
                if let Some(mut resolved) = resolve_include_stmt(
                    &stmt,
                    path,
                    *once,
                    *required,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                    false,
                )? {
                    // A top-level `return` in the included file ends the FILE, not the caller.
                    discard_first_include_return(&mut resolved);
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
                            exception_type_args: catch_clause.exception_type_args.clone(),
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
            StmtKind::FunctionDecl {
                name,
                type_params,
                params,
                param_attributes,
                variadic,
                variadic_by_ref,
                variadic_type,
                return_type,
                by_ref_return,
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
                result.push(Stmt::with_attributes(
                    StmtKind::FunctionDecl {
                        name: name.clone(),
                        type_params: type_params.clone(),
                        params: params.clone(),
                        param_attributes: param_attributes.clone(),
                        variadic: variadic.clone(),
                        variadic_by_ref: *variadic_by_ref,
                        variadic_type: variadic_type.clone(),
                        return_type: return_type.clone(),
                        by_ref_return: *by_ref_return,
                        body,
                    },
                    stmt.span,
                    stmt.attributes.clone(),
                ));
            }
            StmtKind::ClassDecl {
                generics,
                name,
                doc_comment,
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
                        generics: generics.clone(),
                        name: name.clone(),
                        doc_comment: doc_comment.clone(),
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
            StmtKind::InterfaceDecl { generics, name, extends, properties, methods,
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
                        generics: generics.clone(),
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
                generics,
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
                        generics: generics.clone(),
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
