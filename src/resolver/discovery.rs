use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::names::canonical_name_for_decl;
use crate::parser::ast::{
    CatchClause, ClassMethod, ClassProperty, Expr, ExprKind, Stmt, StmtKind,
};

use super::declarations::extract_discoverable_declarations;
use super::engine::resolve_stmts;
use super::files::{parse_file, resolve_path};
use super::include_path::fold_include_path;
use super::state::{
    is_define_call_name, namespace_string, normalize_defined_constant_name,
    register_const_imports, ResolveState,
};

pub(super) fn discover_include_declarations(
    stmts: &[Stmt],
    base_dir: &Path,
) -> Result<Vec<Stmt>, CompileError> {
    let mut declarations = Vec::new();
    let mut loaded_paths = HashSet::new();
    let mut include_chain = Vec::new();
    let mut state = ResolveState::default();

    discover_stmts(
        stmts,
        base_dir,
        &mut loaded_paths,
        &mut include_chain,
        &mut state,
        &mut declarations,
    )?;

    Ok(declarations)
}

fn discover_stmts(
    stmts: &[Stmt],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    declarations: &mut Vec<Stmt>,
) -> Result<(), CompileError> {
    for stmt in stmts {
        discover_stmt(stmt, base_dir, loaded_paths, include_chain, state, declarations)?;
    }
    Ok(())
}

fn discover_stmt(
    stmt: &Stmt,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    declarations: &mut Vec<Stmt>,
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
                declarations,
            )?;
        }
        StmtKind::ConstDecl { name, value } => {
            discover_expr(value, base_dir, loaded_paths, include_chain, state, declarations)?;
            if let Ok(s) = fold_include_path(value, state) {
                let key = canonical_name_for_decl(state.namespace.as_deref(), name);
                state.constants.insert(key, s);
            }
        }
        StmtKind::ExprStmt(expr) => {
            discover_expr(expr, base_dir, loaded_paths, include_chain, state, declarations)?;
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
            discover_stmts(body, base_dir, loaded_paths, include_chain, state, declarations)?;
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
                declarations,
            )?;
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            discover_expr(condition, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_isolated(
                then_body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                declarations,
            )?;
            for (condition, body) in elseif_clauses {
                discover_expr(condition, base_dir, loaded_paths, include_chain, state, declarations)?;
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    declarations,
                )?;
            }
            if let Some(body) = else_body {
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    declarations,
                )?;
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            discover_expr(condition, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_isolated(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                declarations,
            )?;
        }
        StmtKind::For { init, condition, update, body } => {
            if let Some(init) = init {
                discover_stmt(init, base_dir, loaded_paths, include_chain, state, declarations)?;
            }
            if let Some(condition) = condition {
                discover_expr(condition, base_dir, loaded_paths, include_chain, state, declarations)?;
            }
            if let Some(update) = update {
                discover_stmt(update, base_dir, loaded_paths, include_chain, state, declarations)?;
            }
            discover_isolated(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                declarations,
            )?;
        }
        StmtKind::Foreach { array, body, .. } => {
            discover_expr(array, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_isolated(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                declarations,
            )?;
        }
        StmtKind::Switch { subject, cases, default } => {
            discover_expr(subject, base_dir, loaded_paths, include_chain, state, declarations)?;
            for (values, body) in cases {
                for value in values {
                    discover_expr(value, base_dir, loaded_paths, include_chain, state, declarations)?;
                }
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    declarations,
                )?;
            }
            if let Some(body) = default {
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    declarations,
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
                declarations,
            )?;
            for CatchClause { body, .. } in catches {
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    declarations,
                )?;
            }
            if let Some(body) = finally_body {
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    declarations,
                )?;
            }
        }
        StmtKind::FunctionDecl { params, body, .. } => {
            discover_params(params, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_isolated(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                declarations,
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
                declarations,
            )?;
            discover_methods(
                methods,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                declarations,
            )?;
        }
        StmtKind::InterfaceDecl { methods, .. } => {
            discover_methods(
                methods,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                declarations,
            )?;
        }
        StmtKind::EnumDecl { cases, .. } => {
            for case in cases {
                if let Some(value) = &case.value {
                    discover_expr(value, base_dir, loaded_paths, include_chain, state, declarations)?;
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
            discover_expr(expr, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            discover_expr(index, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_expr(value, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_expr(value, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        StmtKind::Return(None)
        | StmtKind::IncludeOnceMark { .. }
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

#[allow(clippy::too_many_arguments)]
fn discover_include(
    path: &Expr,
    once: bool,
    required: bool,
    span: crate::span::Span,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    declarations: &mut Vec<Stmt>,
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
    discover_stmts(
        &included_stmts,
        included_dir,
        loaded_paths,
        include_chain,
        state,
        declarations,
    )?;
    state.namespace = saved_namespace;
    state.const_imports = saved_imports;

    let mut declaration_declared_once = HashSet::new();
    let mut declaration_include_chain = include_chain.clone();
    let resolved_declarations = resolve_stmts(
        included_stmts.clone(),
        included_dir,
        &mut declaration_declared_once,
        &mut declaration_include_chain,
        &mut declaration_state,
    )?;

    include_chain.pop();
    loaded_paths.insert(canonical);

    let file_declarations = extract_discoverable_declarations(&resolved_declarations);
    if !file_declarations.is_empty() {
        declarations.push(Stmt::new(
            StmtKind::NamespaceBlock {
                name: None,
                body: file_declarations,
            },
            span,
        ));
    }

    Ok(())
}

fn discover_isolated(
    stmts: &[Stmt],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    declarations: &mut Vec<Stmt>,
) -> Result<(), CompileError> {
    let mut local = state.clone();
    discover_stmts(
        stmts,
        base_dir,
        loaded_paths,
        include_chain,
        &mut local,
        declarations,
    )
}

fn discover_params(
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    declarations: &mut Vec<Stmt>,
) -> Result<(), CompileError> {
    for (_, _, default, _) in params {
        if let Some(default) = default {
            discover_expr(default, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
    }
    Ok(())
}

fn discover_properties(
    properties: &[ClassProperty],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    declarations: &mut Vec<Stmt>,
) -> Result<(), CompileError> {
    for property in properties {
        if let Some(default) = &property.default {
            discover_expr(default, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
    }
    Ok(())
}

fn discover_methods(
    methods: &[ClassMethod],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    declarations: &mut Vec<Stmt>,
) -> Result<(), CompileError> {
    for method in methods {
        let mut local = state.clone();
        discover_params(
            &method.params,
            base_dir,
            loaded_paths,
            include_chain,
            &mut local,
            declarations,
        )?;
        discover_isolated(
            &method.body,
            base_dir,
            loaded_paths,
            include_chain,
            state,
            declarations,
        )?;
    }
    Ok(())
}

fn discover_expr(
    expr: &Expr,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    declarations: &mut Vec<Stmt>,
) -> Result<(), CompileError> {
    match &expr.kind {
        ExprKind::BinaryOp { left, right, .. } => {
            discover_expr(left, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_expr(right, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        ExprKind::InstanceOf { value, .. }
        | ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Spread(value)
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::BufferNew { len: value, .. } => {
            discover_expr(value, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ArrayAccess { array: value, index: default }
        | ExprKind::ShortTernary { value, default } => {
            discover_expr(value, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_expr(default, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        ExprKind::Assignment { target, value, result_target, prelude, .. } => {
            discover_expr(target, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_expr(value, base_dir, loaded_paths, include_chain, state, declarations)?;
            if let Some(result_target) = result_target {
                discover_expr(
                    result_target,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    declarations,
                )?;
            }
            discover_isolated(
                prelude,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                declarations,
            )?;
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            discover_exprs(args, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        ExprKind::ArrayLiteral(items) => {
            discover_exprs(items, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                discover_expr(key, base_dir, loaded_paths, include_chain, state, declarations)?;
                discover_expr(value, base_dir, loaded_paths, include_chain, state, declarations)?;
            }
        }
        ExprKind::Match { subject, arms, default } => {
            discover_expr(subject, base_dir, loaded_paths, include_chain, state, declarations)?;
            for (patterns, value) in arms {
                discover_exprs(
                    patterns,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    declarations,
                )?;
                discover_expr(value, base_dir, loaded_paths, include_chain, state, declarations)?;
            }
            if let Some(default) = default {
                discover_expr(default, base_dir, loaded_paths, include_chain, state, declarations)?;
            }
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            discover_expr(condition, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_expr(then_expr, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_expr(else_expr, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        ExprKind::Cast { expr, .. } | ExprKind::NamedArg { value: expr, .. } => {
            discover_expr(expr, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        ExprKind::Closure { params, body, .. } => {
            discover_params(params, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_isolated(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                declarations,
            )?;
        }
        ExprKind::ExprCall { callee, args } => {
            discover_expr(callee, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_exprs(args, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, declarations)?;
            discover_exprs(args, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        ExprKind::FirstClassCallable(crate::parser::ast::CallableTarget::Method { object, .. }) => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, declarations)?;
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::MagicConstant(_) => {}
    }
    Ok(())
}

fn discover_exprs(
    exprs: &[Expr],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    declarations: &mut Vec<Stmt>,
) -> Result<(), CompileError> {
    for expr in exprs {
        discover_expr(expr, base_dir, loaded_paths, include_chain, state, declarations)?;
    }
    Ok(())
}
