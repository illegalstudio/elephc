use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::lexer;
use crate::parser;
use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::span::Span;

/// Resolves all include/require statements by inlining the referenced files.
/// Runs between parsing and type checking.
pub fn resolve(program: Program, base_dir: &Path) -> Result<Program, CompileError> {
    // Fast path: if no includes exist anywhere, return as-is
    if !has_includes(&program) {
        return Ok(program);
    }

    let mut included: HashSet<PathBuf> = HashSet::new();
    let mut include_chain: Vec<PathBuf> = Vec::new();
    resolve_stmts(program, base_dir, &mut included, &mut include_chain)
}

/// Check if any statement (recursively) contains an Include.
fn has_includes(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| match &stmt.kind {
        StmtKind::Include { .. } => true,
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            has_includes(then_body)
                || elseif_clauses.iter().any(|(_, body)| has_includes(body))
                || else_body.as_ref().is_some_and(|b| has_includes(b))
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::FunctionDecl { body, .. } => has_includes(body),
        StmtKind::ConstDecl { .. } | StmtKind::ListUnpack { .. }
        | StmtKind::Global { .. } | StmtKind::StaticVar { .. } => false,
        StmtKind::Switch { cases, default, .. } => {
            cases.iter().any(|(_, body)| has_includes(body))
                || default.as_ref().is_some_and(|b| has_includes(b))
        }
        _ => false,
    })
}

fn resolve_stmts(
    stmts: Vec<Stmt>,
    base_dir: &Path,
    included: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
) -> Result<Vec<Stmt>, CompileError> {
    let mut result = Vec::new();

    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Include { path, once, required } => {
                let resolved = resolve_path(path, base_dir);
                let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

                // Check if file exists
                if !resolved.exists() {
                    if *required {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!("Required file not found: '{}'", path),
                        ));
                    }
                    continue;
                }

                // Handle _once: skip if already included
                if *once && included.contains(&canonical) {
                    continue;
                }

                // Detect circular includes
                if include_chain.contains(&canonical) {
                    return Err(CompileError::new(
                        stmt.span,
                        &format!("Circular include detected: '{}'", path),
                    ));
                }

                included.insert(canonical.clone());

                // Read, tokenize, parse the included file
                let included_stmts = parse_file(&resolved, stmt.span)?;

                // Recursively resolve includes in the included file
                let included_dir = resolved.parent().unwrap_or(base_dir);
                include_chain.push(canonical);
                let resolved_stmts =
                    resolve_stmts(included_stmts, included_dir, included, include_chain)?;
                include_chain.pop();

                result.extend(resolved_stmts);
            }
            // Recurse into bodies that can contain statements
            StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
                let then_resolved =
                    resolve_stmts(then_body.clone(), base_dir, included, include_chain)?;
                let mut elseif_resolved = Vec::new();
                for (cond, body) in elseif_clauses {
                    let body_resolved =
                        resolve_stmts(body.clone(), base_dir, included, include_chain)?;
                    elseif_resolved.push((cond.clone(), body_resolved));
                }
                let else_resolved = if let Some(body) = else_body {
                    Some(resolve_stmts(body.clone(), base_dir, included, include_chain)?)
                } else {
                    None
                };
                result.push(Stmt::new(
                    StmtKind::If {
                        condition: condition.clone(),
                        then_body: then_resolved,
                        elseif_clauses: elseif_resolved,
                        else_body: else_resolved,
                    },
                    stmt.span,
                ));
            }
            StmtKind::While { condition, body } => {
                let body_resolved =
                    resolve_stmts(body.clone(), base_dir, included, include_chain)?;
                result.push(Stmt::new(
                    StmtKind::While { condition: condition.clone(), body: body_resolved },
                    stmt.span,
                ));
            }
            StmtKind::DoWhile { body, condition } => {
                let body_resolved =
                    resolve_stmts(body.clone(), base_dir, included, include_chain)?;
                result.push(Stmt::new(
                    StmtKind::DoWhile { body: body_resolved, condition: condition.clone() },
                    stmt.span,
                ));
            }
            StmtKind::For { init, condition, update, body } => {
                let body_resolved =
                    resolve_stmts(body.clone(), base_dir, included, include_chain)?;
                result.push(Stmt::new(
                    StmtKind::For {
                        init: init.clone(),
                        condition: condition.clone(),
                        update: update.clone(),
                        body: body_resolved,
                    },
                    stmt.span,
                ));
            }
            StmtKind::Foreach { array, key_var, value_var, body } => {
                let body_resolved =
                    resolve_stmts(body.clone(), base_dir, included, include_chain)?;
                result.push(Stmt::new(
                    StmtKind::Foreach {
                        array: array.clone(),
                        key_var: key_var.clone(),
                        value_var: value_var.clone(),
                        body: body_resolved,
                    },
                    stmt.span,
                ));
            }
            StmtKind::Switch { subject, cases, default } => {
                let mut cases_resolved = Vec::new();
                for (values, body) in cases {
                    let body_resolved =
                        resolve_stmts(body.clone(), base_dir, included, include_chain)?;
                    cases_resolved.push((values.clone(), body_resolved));
                }
                let default_resolved = if let Some(body) = default {
                    Some(resolve_stmts(body.clone(), base_dir, included, include_chain)?)
                } else {
                    None
                };
                result.push(Stmt::new(
                    StmtKind::Switch {
                        subject: subject.clone(),
                        cases: cases_resolved,
                        default: default_resolved,
                    },
                    stmt.span,
                ));
            }
            StmtKind::FunctionDecl { name, params, variadic, body } => {
                let body_resolved =
                    resolve_stmts(body.clone(), base_dir, included, include_chain)?;
                result.push(Stmt::new(
                    StmtKind::FunctionDecl {
                        name: name.clone(),
                        params: params.clone(),
                        variadic: variadic.clone(),
                        body: body_resolved,
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

    let tokens = lexer::tokenize(&source).map_err(|e| {
        CompileError::new(
            include_span,
            &format!("Error in '{}': {}", path.display(), e.message),
        )
    })?;

    parser::parse(&tokens).map_err(|e| {
        CompileError::new(
            include_span,
            &format!("Error in '{}': {}", path.display(), e.message),
        )
    })
}
