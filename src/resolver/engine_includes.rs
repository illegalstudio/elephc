//! Purpose:
//! Resolves individual include and require statements during resolver traversal.
//! Parses target files, handles include_once state, and merges resolved included statements.
//!
//! Called from:
//! - `crate::resolver::engine::resolve_stmts()`.
//!
//! Key details:
//! - Include paths are folded in the caller's constant state and file base directory.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

use super::declarations::strip_discoverable_declarations;
use super::discovery::FunctionVariantRegistry;
use super::engine::resolve_stmts;
use super::files::{parse_file, resolve_path};
use super::include_once::include_once_label;
use super::include_path::fold_include_path;
use super::state::ResolveState;

/// Process-global counter producing unique hidden temporary names for value-position includes.
static VALUE_INCLUDE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Where the value produced by an expression-position include must be delivered.
pub(super) enum IncludeValueCapture {
    /// `$name = require X;` — assign the include's value to the named caller variable.
    Assign(String),
    /// `return require X;` — return the include's value from the enclosing function.
    Return,
}

/// Resolves a single include/require statement by parsing the target file,
/// recursively resolving its statements, and returning them wrapped in
/// appropriate include_once guards.
///
/// - `once`: when true, skips already-included files and wraps output in `IncludeOnceGuard`
/// - `required`: when true, returns an error if the target file does not exist
/// - `declared_once`: tracks files already processed; updated on return
/// - `include_chain`: current include path for cycle detection; must not contain `canonical`
/// - State (`namespace`, `const_imports`) is saved before recursion and restored after
/// - Returns `None` if the file does not exist and `required` is false, or if a once file was already included
/// - For `once`: wraps body in `IncludeOnceGuard` with the file's label
/// - For non-once: emits `IncludeOnceMark` before the body for later once/require_once checks
pub(super) fn resolve_include_stmt(
    stmt: &Stmt,
    path: &Expr,
    once: bool,
    required: bool,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<Option<Vec<Stmt>>, CompileError> {
    let path_str =
        fold_include_path(path, state).map_err(|msg| CompileError::new(stmt.span, &msg))?;
    let resolved = resolve_path(&path_str, base_dir);
    let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

    if !resolved.exists() {
        if required {
            return Err(CompileError::new(
                stmt.span,
                &format!("Required file not found: '{}'", path_str),
            ));
        }
        return Ok(None);
    }

    if include_chain.contains(&canonical) {
        if once {
            return Ok(None);
        }
        return Err(CompileError::new(
            stmt.span,
            &format!("Circular include detected: '{}'", path_str),
        ));
    }

    let included_stmts = parse_file(&resolved, stmt.span)?;
    let included_stmts =
        crate::magic_constants::substitute_file_and_scope_constants(included_stmts, &resolved);
    // Strict-PHP audit of the included user file on its freshly parsed AST,
    // before include-variant synthesis introduces compiler-internal names.
    crate::strict_php::check_file(&included_stmts, &resolved.display().to_string())?;

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
    let executable =
        strip_discoverable_declarations(resolved_stmts, Some(&canonical), function_variants);
    if once {
        // Declaration discovery already hoisted compile-time declarations;
        // executable include body statements are guarded so runtime order matches PHP.
        declared_once.insert(canonical);
        return Ok(Some(vec![Stmt::new(
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
        )]));
    }

    // Regular includes still mark the file as loaded for a later
    // include_once/require_once, while executable statements stay at
    // the include point.
    declared_once.insert(canonical);
    Ok(Some(vec![
        Stmt::new(
            StmtKind::IncludeOnceMark {
                label: include_label,
            },
            stmt.span,
        ),
        Stmt::new(
            StmtKind::NamespaceBlock {
                name: None,
                body: executable,
            },
            stmt.span,
        ),
    ]))
}

/// Expands an expression-position `include`/`require` (`$x = require X;` or `return require X;`)
/// into a sequence of statements that run the included file *in the caller's scope* and deliver
/// its value to `capture`.
///
/// The included file's statements are inlined directly (sharing the caller's variables), and its
/// first top-level `return E` is rewritten to assign a hidden temporary. A successful include with
/// no top-level `return` yields `1`; a missing non-required include yields `false`, matching PHP.
///
/// Nested top-level returns inside control flow within the included file are not rewritten and keep
/// the same semantics as a statement-position include (they return from the enclosing function).
pub(super) fn expand_value_include(
    span: Span,
    path: &Expr,
    once: bool,
    required: bool,
    capture: IncludeValueCapture,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<Vec<Stmt>, CompileError> {
    let tmp = format!(
        "__elephc_inc_{}",
        VALUE_INCLUDE_COUNTER.fetch_add(1, Ordering::Relaxed)
    );

    let include_stmt = Stmt::new(
        StmtKind::Include {
            path: path.clone(),
            once,
            required,
        },
        span,
    );
    let resolved = resolve_include_stmt(
        &include_stmt,
        path,
        once,
        required,
        base_dir,
        declared_once,
        include_chain,
        state,
        function_variants,
    )?;

    let mut out = Vec::new();
    match resolved {
        // Missing, non-required include: PHP evaluates the expression to `false`.
        None => {
            out.push(assign_temp(
                &tmp,
                Expr::new(ExprKind::BoolLiteral(false), span),
                span,
            ));
        }
        Some(mut wrapped) => {
            let captured_return = rewrite_first_include_return(&mut wrapped, &tmp);
            // Pre-seed the default include value of `1` when the included body cannot set the
            // temporary itself: either it has no top-level `return`, or it is an `_once` include
            // whose guarded body may be skipped on a repeat include.
            if !captured_return || once {
                out.push(assign_temp(
                    &tmp,
                    Expr::new(ExprKind::IntLiteral(1), span),
                    span,
                ));
            }
            out.extend(wrapped);
        }
    }

    let value = Expr::new(ExprKind::Variable(tmp), span);
    match capture {
        IncludeValueCapture::Assign(name) => {
            out.push(Stmt::new(StmtKind::Assign { name, value }, span));
        }
        IncludeValueCapture::Return => {
            out.push(Stmt::new(StmtKind::Return(Some(value)), span));
        }
    }
    Ok(out)
}

/// Builds a `<temp> = <value>;` assignment statement for the hidden include temporary.
fn assign_temp(temp: &str, value: Expr, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::Assign {
            name: temp.to_string(),
            value,
        },
        span,
    )
}

/// Rewrites the first top-level `return` inside the wrapped include body to assign the include
/// temporary, dropping any statements after it (they are unreachable once the include returns).
///
/// Recurses through the `IncludeOnceGuard`/`NamespaceBlock` wrappers produced by
/// `resolve_include_stmt`. Returns `true` if a top-level `return` was found and rewritten.
fn rewrite_first_include_return(wrapped: &mut [Stmt], temp: &str) -> bool {
    for stmt in wrapped.iter_mut() {
        match &mut stmt.kind {
            StmtKind::NamespaceBlock { body, .. } => {
                if rewrite_top_level_return(body, temp) {
                    return true;
                }
            }
            StmtKind::IncludeOnceGuard { body, .. } => {
                if rewrite_first_include_return(body, temp) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Replaces the first top-level `return E;` in `body` with `<temp> = E;` (or drops a bare
/// `return;`, leaving the temporary at its default) and truncates the now-unreachable tail.
/// Returns `true` if a top-level `return` was rewritten.
fn rewrite_top_level_return(body: &mut Vec<Stmt>, temp: &str) -> bool {
    for i in 0..body.len() {
        if matches!(body[i].kind, StmtKind::Return(_)) {
            let span = body[i].span;
            let placeholder = Stmt::new(StmtKind::Return(None), span);
            let original = std::mem::replace(&mut body[i], placeholder);
            if let StmtKind::Return(Some(value)) = original.kind {
                body[i] = assign_temp(temp, value, span);
            } else {
                // Bare `return;` carries no value; leave the temporary at its default and drop the
                // statement by replacing it with an empty sequence.
                body[i] = Stmt::new(StmtKind::Synthetic(Vec::new()), span);
            }
            body.truncate(i + 1);
            return true;
        }
    }
    false
}
