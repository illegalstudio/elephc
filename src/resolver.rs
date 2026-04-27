use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::lexer;
use crate::names::{canonical_name_for_decl, Name, NameKind};
use crate::parser;
use crate::parser::ast::{
    BinOp, CatchClause, ClassMethod, Expr, ExprKind, Program, Stmt, StmtKind, UseKind,
};
use crate::span::Span;

#[derive(Clone, Default)]
struct ResolveState {
    constants: HashMap<String, String>,
    namespace: Option<String>,
    const_imports: HashMap<String, String>,
}

/// Resolves all include/require statements by inlining the referenced files.
/// Runs between parsing and type checking.
pub fn resolve(program: Program, base_dir: &Path) -> Result<Program, CompileError> {
    if !has_includes(&program) {
        return Ok(program);
    }

    let mut included: HashSet<PathBuf> = HashSet::new();
    let mut include_chain: Vec<PathBuf> = Vec::new();
    let mut state = ResolveState::default();
    resolve_stmts(program, base_dir, &mut included, &mut include_chain, &mut state)
}

/// Fold a path expression to a compile-time string. Handles string literals,
/// concat of foldable subexpressions, and references to const/define-d string
/// constants tracked in `state`. Returns the human-readable error message when
/// the expression cannot be folded.
fn fold_include_path(expr: &Expr, state: &ResolveState) -> Result<String, String> {
    match &expr.kind {
        ExprKind::StringLiteral(s) => Ok(s.clone()),
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => {
            let l = fold_include_path(left, state)?;
            let r = fold_include_path(right, state)?;
            Ok(l + &r)
        }
        ExprKind::ConstRef(name) => resolve_constant_ref(name, state).ok_or_else(|| {
            format!(
                "include path references unknown constant '{}'; \
                 the constant must be defined (via `const` or `define()`) \
                 before the include statement",
                name.as_str()
            )
        }),
        _ => Err(
            "include path must be a compile-time-constant string \
             (string literal, concatenation thereof, or a `const`/`define()`-d \
             string constant)"
                .to_string(),
        ),
    }
}

fn resolve_constant_ref(name: &Name, state: &ResolveState) -> Option<String> {
    constant_lookup_candidates(name, state)
        .into_iter()
        .find_map(|candidate| state.constants.get(&candidate).cloned())
}

fn constant_lookup_candidates(name: &Name, state: &ResolveState) -> Vec<String> {
    if name.is_fully_qualified() {
        return vec![name.as_canonical()];
    }

    if name.is_unqualified() {
        if let Some(alias) = name
            .last_segment()
            .and_then(|segment| state.const_imports.get(segment))
        {
            return vec![alias.clone()];
        }

        let raw = name.as_canonical();
        if let Some(namespace) = state.namespace.as_deref() {
            if !namespace.is_empty() {
                return vec![format!("{}\\{}", namespace, raw), raw];
            }
        }
        return vec![raw];
    }

    if let Some(first) = name.parts.first() {
        if let Some(alias) = state.const_imports.get(first) {
            let suffix = &name.parts[1..];
            if suffix.is_empty() {
                return vec![alias.clone()];
            }
            return vec![format!("{}\\{}", alias, suffix.join("\\"))];
        }
    }

    let raw = name.as_canonical();
    if name.kind == NameKind::Qualified {
        if let Some(namespace) = state.namespace.as_deref() {
            if !namespace.is_empty() {
                return vec![format!("{}\\{}", namespace, raw)];
            }
        }
    }
    vec![raw]
}

fn normalize_defined_constant_name(name: &str) -> String {
    name.trim_start_matches('\\').to_string()
}

fn namespace_string(name: &Option<Name>) -> String {
    name.as_ref().map(Name::as_canonical).unwrap_or_default()
}

fn register_const_imports(state: &mut ResolveState, stmt: &Stmt) {
    let StmtKind::UseDecl { imports } = &stmt.kind else {
        return;
    };
    for item in imports {
        if item.kind == UseKind::Const {
            state.const_imports.insert(
                item.alias.clone(),
                normalize_defined_constant_name(&item.name.as_canonical()),
            );
        }
    }
}

fn is_define_call_name(name: &Name) -> bool {
    matches!(name.kind, NameKind::Unqualified | NameKind::FullyQualified)
        && name.parts.len() == 1
        && name.parts[0] == "define"
}

/// Check if any statement (recursively) contains an Include.
fn has_includes(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| match &stmt.kind {
        StmtKind::Include { .. } => true,
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            has_includes(then_body)
                || elseif_clauses.iter().any(|(_, body)| has_includes(body))
                || else_body.as_ref().is_some_and(|body| has_includes(body))
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::FunctionDecl { body, .. }
        | StmtKind::NamespaceBlock { body, .. } => has_includes(body),
        StmtKind::Try { try_body, catches, finally_body } => {
            has_includes(try_body)
                || catches.iter().any(|catch_clause| has_includes(&catch_clause.body))
                || finally_body.as_ref().is_some_and(|body| has_includes(body))
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. } => methods.iter().any(|m| has_includes(&m.body)),
        StmtKind::Switch { cases, default, .. } => {
            cases.iter().any(|(_, body)| has_includes(body))
                || default.as_ref().is_some_and(|body| has_includes(body))
        }
        _ => false,
    })
}

fn resolve_stmts(
    stmts: Vec<Stmt>,
    base_dir: &Path,
    included: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
) -> Result<Vec<Stmt>, CompileError> {
    let mut result = Vec::new();

    for stmt in stmts {
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

                if *once && included.contains(&canonical) {
                    continue;
                }

                if include_chain.contains(&canonical) {
                    return Err(CompileError::new(
                        stmt.span,
                        &format!("Circular include detected: '{}'", path_str),
                    ));
                }

                included.insert(canonical.clone());

                let included_stmts = parse_file(&resolved, stmt.span)?;
                let included_stmts =
                    crate::magic_constants::substitute_file_and_scope_constants(included_stmts, &resolved);

                let included_dir = resolved.parent().unwrap_or(base_dir);
                include_chain.push(canonical);

                let saved_namespace = state.namespace.clone();
                let saved_imports = state.const_imports.clone();
                state.namespace = None;
                state.const_imports = HashMap::new();
                let resolved_stmts =
                    resolve_stmts(included_stmts, included_dir, included, include_chain, state)?;
                state.namespace = saved_namespace;
                state.const_imports = saved_imports;

                include_chain.pop();

                result.push(Stmt::new(
                    StmtKind::NamespaceBlock {
                        name: None,
                        body: resolved_stmts,
                    },
                    stmt.span,
                ));
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
                let body_resolved =
                    resolve_stmts(body.clone(), base_dir, included, include_chain, state)?;
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
                let then_body = resolve_isolated(then_body.clone(), base_dir, included, include_chain, state)?;
                let elseif_clauses = elseif_clauses
                    .iter()
                    .map(|(cond, body)| {
                        Ok((
                            cond.clone(),
                            resolve_isolated(body.clone(), base_dir, included, include_chain, state)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?;
                let else_body = else_body
                    .as_ref()
                    .map(|body| resolve_isolated(body.clone(), base_dir, included, include_chain, state))
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
                let body = resolve_isolated(body.clone(), base_dir, included, include_chain, state)?;
                result.push(Stmt::new(
                    StmtKind::While {
                        condition: condition.clone(),
                        body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::DoWhile { body, condition } => {
                let body = resolve_isolated(body.clone(), base_dir, included, include_chain, state)?;
                result.push(Stmt::new(
                    StmtKind::DoWhile {
                        body,
                        condition: condition.clone(),
                    },
                    stmt.span,
                ));
            }
            StmtKind::For { init, condition, update, body } => {
                let body = resolve_isolated(body.clone(), base_dir, included, include_chain, state)?;
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
                let body = resolve_isolated(body.clone(), base_dir, included, include_chain, state)?;
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
                            resolve_isolated(body.clone(), base_dir, included, include_chain, state)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?;
                let default = default
                    .as_ref()
                    .map(|body| resolve_isolated(body.clone(), base_dir, included, include_chain, state))
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
                let try_body =
                    resolve_isolated(try_body.clone(), base_dir, included, include_chain, state)?;
                let catches = catches
                    .iter()
                    .map(|catch_clause| {
                        Ok(CatchClause {
                            exception_types: catch_clause.exception_types.clone(),
                            variable: catch_clause.variable.clone(),
                            body: resolve_isolated(
                                catch_clause.body.clone(),
                                base_dir,
                                included,
                                include_chain,
                                state,
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?;
                let finally_body = finally_body
                    .as_ref()
                    .map(|body| resolve_isolated(body.clone(), base_dir, included, include_chain, state))
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
                let body = resolve_isolated(body.clone(), base_dir, included, include_chain, state)?;
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
                let methods = resolve_methods(methods, base_dir, included, include_chain, state)?;
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
                let methods = resolve_methods(methods, base_dir, included, include_chain, state)?;
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
                let methods = resolve_methods(methods, base_dir, included, include_chain, state)?;
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

fn resolve_isolated(
    stmts: Vec<Stmt>,
    base_dir: &Path,
    included: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<Vec<Stmt>, CompileError> {
    let mut local = state.clone();
    resolve_stmts(stmts, base_dir, included, include_chain, &mut local)
}

fn resolve_methods(
    methods: &[ClassMethod],
    base_dir: &Path,
    included: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<Vec<ClassMethod>, CompileError> {
    methods
        .iter()
        .map(|method| {
            let body =
                resolve_isolated(method.body.clone(), base_dir, included, include_chain, state)?;
            Ok(ClassMethod {
                body,
                ..method.clone()
            })
        })
        .collect()
}

fn resolve_path(path: &str, base_dir: &Path) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    }
}

fn parse_file(path: &Path, include_span: Span) -> Result<Vec<Stmt>, CompileError> {
    let source = std::fs::read_to_string(path).map_err(|e| {
        CompileError::new(
            include_span,
            &format!("Cannot read '{}': {}", path.display(), e),
        )
    })?;

    let file = path.display().to_string();

    let tokens = lexer::tokenize(&source).map_err(|e| e.with_file(file.clone()))?;

    parser::parse(&tokens).map_err(|e| e.with_file(file))
}
