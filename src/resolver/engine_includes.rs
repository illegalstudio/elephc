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
    if include_chain.len() >= super::MAX_INCLUDE_DEPTH {
        return Err(CompileError::new(
            stmt.span,
            "maximum include depth exceeded",
        ));
    }
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

    let included_stmts =
        parse_file(&resolved, stmt.span, &state.conditional_defines)?;

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
/// A `return` nested in the file's own control flow — an `if`, a loop, a `switch`, a `try` — ends
/// THAT file too: `confine_nested_returns` wraps the body in `do { … } while (false)` and turns each
/// such `return E` into an assignment to the temporary plus a `break` out to the wrapper, and a bare
/// `return;` yields NULL.
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

/// Stops a STATEMENT-position include at its first top-level `return`, discarding the value.
///
/// PHP's `return` inside an included file ends THAT FILE and hands control back to the line
/// after the include; it does not return from whatever function the include sits in, and at
/// the top level it certainly does not end the program. elephc inlines the include body, so
/// without this rewrite the inlined `return` is read as a return from the enclosing function —
/// `main` for a top-level include — and every statement after the include is silently dropped.
///
/// `opcache.preload` is what made this urgent rather than theoretical. The directive injects a
/// `require_once` in statement position, and a preload file ending in `return` is ordinary (it
/// is how a file says "nothing more to do here"); under reference PHP that is harmless, so a
/// preload that truncates the whole program is a divergence with no warning attached to it.
///
/// A `return` NESTED in control flow is handled by [`confine_nested_returns`], which wraps the
/// body in `do { … } while (false)` and turns each in-scope `return` into `break <n>`. Only a
/// body whose returns are all at the top level reaches the static truncation below.
///
/// The value-capturing form (`$x = require F;`) has always done this through
/// `rewrite_first_include_return`, which assigns the returned value to a temporary. The only
/// difference here is that nobody wants the value — but `return foo();` must still CALL
/// `foo()`, so the statement becomes an `ExprStmt` rather than being dropped.
///
/// Declarations are unaffected: `strip_discoverable_declarations` has already hoisted every
/// compile-time declaration out of this body, including those written after the `return`, which
/// is what php-src's early binding does too.
pub(super) fn discard_first_include_return(wrapped: &mut [Stmt]) -> bool {
    for stmt in wrapped.iter_mut() {
        match &mut stmt.kind {
            StmtKind::NamespaceBlock { body, .. } => {
                if confine_nested_returns(body, None) || discard_top_level_return(body) {
                    return true;
                }
            }
            StmtKind::IncludeOnceGuard { body, .. } => {
                if discard_first_include_return(body) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Replaces the first top-level `return E;` in `body` with `E;` (or drops a bare `return;`) and
/// truncates the now-unreachable tail. Returns `true` if a top-level `return` was rewritten.
fn discard_top_level_return(body: &mut Vec<Stmt>) -> bool {
    for i in 0..body.len() {
        if matches!(body[i].kind, StmtKind::Return(_)) {
            let span = body[i].span;
            let placeholder = Stmt::new(StmtKind::Return(None), span);
            let original = std::mem::replace(&mut body[i], placeholder);
            body[i] = match original.kind {
                // The value is discarded, but evaluating it is observable.
                StmtKind::Return(Some(value)) => Stmt::new(StmtKind::ExprStmt(value), span),
                _ => Stmt::new(StmtKind::Synthetic(Vec::new()), span),
            };
            body.truncate(i + 1);
            return true;
        }
    }
    false
}

/// Confines every `return` in an included file's OWN scope to that file, when one of them is
/// nested in control flow. Returns `true` when it rewrote the body.
///
/// A `return` inside an `if`, a loop, a `switch` or a `try` still ended the CALLER: the include
/// body is inlined, and only a direct top-level `return` was rewritten. `if (!defined('X')) {
/// return; }` is the classic include guard, and a conditional return in `opcache.preload` ended
/// the whole program before the entry script ran. MEASURED (PR review): with `REVIEW_STOP=1` the
/// preload `if (getenv(...) === "1") { return; }` made elephc print only `PRELOAD`; reference
/// prints `PRELOAD` then `MAIN`.
///
/// A conditional return cannot become a static truncation, so the body is wrapped in
/// `do { … } while (false)` and each in-scope `return` becomes `break <n>`, `n` counting the
/// loops and switches between it and the wrapper. The walk recurses through control flow but
/// never into a function, class, enum, interface or trait body, whose `return` is its own; a
/// closure is an expression and is never reached. A nested include was confined when it was
/// resolved, before this file, so no `return` of its own is left for this walk to take. `break`
/// through a `finally` runs the `finally`, as the `return` it replaces did.
///
/// `temp` is the value form's hidden temporary: `return E;` becomes `<temp> = E; break n;`, and a
/// bare `return;` assigns NULL. Without it the value is still EVALUATED, because `return f();`
/// must still call `f()`.
///
/// Only a body with a NESTED return is rewritten. One whose only `return` is at the top level
/// keeps the static truncation, which needs no wrapper.
fn confine_nested_returns(body: &mut Vec<Stmt>, temp: Option<&str>) -> bool {
    if !has_nested_return(body) {
        return false;
    }
    rewrite_scoped_returns(body, 1, temp);
    let span = body.first().map_or_else(Span::dummy, |stmt| stmt.span);
    let inner = std::mem::take(body);
    body.push(Stmt::new(
        StmtKind::DoWhile {
            body: inner,
            condition: Expr::new(ExprKind::BoolLiteral(false), span),
        },
        span,
    ));
    true
}

/// Whether `stmts` holds a `return` of this file's scope below the top level.
fn has_nested_return(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| {
        scoped_child_bodies(stmt)
            .into_iter()
            .any(|(child, _)| child.iter().any(|s| matches!(s.kind, StmtKind::Return(_))) || has_nested_return(child))
    })
}

/// The bodies of `stmt` that belong to the SAME scope, with how many `break` levels each adds.
/// Function, class and other declaration bodies are excluded: their `return` is their own.
fn scoped_child_bodies(stmt: &Stmt) -> Vec<(&Vec<Stmt>, usize)> {
    match &stmt.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut bodies = vec![(then_body, 0)];
            bodies.extend(elseif_clauses.iter().map(|(_, body)| (body, 0)));
            bodies.extend(else_body.iter().map(|body| (body, 0)));
            bodies
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            let mut bodies = vec![(then_body, 0)];
            bodies.extend(else_body.iter().map(|body| (body, 0)));
            bodies
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => vec![(body, 0)],
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let mut bodies = vec![(try_body, 0)];
            bodies.extend(catches.iter().map(|clause| (&clause.body, 0)));
            bodies.extend(finally_body.iter().map(|body| (body, 0)));
            bodies
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => vec![(body, 1)],
        StmtKind::Switch { cases, default, .. } => {
            let mut bodies: Vec<_> = cases.iter().map(|(_, body)| (body, 1)).collect();
            bodies.extend(default.iter().map(|body| (body, 1)));
            bodies
        }
        _ => Vec::new(),
    }
}

/// Rewrites every `return` of this scope in `stmts` into `[<temp> = E; | E;] break <depth>;`,
/// recursing into the same bodies [`scoped_child_bodies`] names.
fn rewrite_scoped_returns(stmts: &mut [Stmt], depth: usize, temp: Option<&str>) {
    for stmt in stmts.iter_mut() {
        let span = stmt.span;
        if let StmtKind::Return(value) = &mut stmt.kind {
            let value = value.take();
            let mut replacement = Vec::new();
            match (temp, value) {
                (Some(temp), Some(value)) => replacement.push(assign_temp(temp, value, span)),
                (Some(temp), None) => {
                    replacement.push(assign_temp(temp, Expr::new(ExprKind::Null, span), span));
                }
                (None, Some(value)) => replacement.push(Stmt::new(StmtKind::ExprStmt(value), span)),
                (None, None) => {}
            }
            // Marked, so the checker lets it leave a `finally`: `finally { return 7; }` is
            // legal PHP, and reference prints `try after:7` for it (MEASURED), where an
            // unmarked `break` was refused as "Cannot jump out of a finally block".
            replacement.push(Stmt::include_return_break(depth, span));
            stmt.kind = StmtKind::Synthetic(replacement);
            continue;
        }
        match &mut stmt.kind {
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                rewrite_scoped_returns(then_body, depth, temp);
                for (_, body) in elseif_clauses.iter_mut() {
                    rewrite_scoped_returns(body, depth, temp);
                }
                if let Some(body) = else_body {
                    rewrite_scoped_returns(body, depth, temp);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                rewrite_scoped_returns(then_body, depth, temp);
                if let Some(body) = else_body {
                    rewrite_scoped_returns(body, depth, temp);
                }
            }
            StmtKind::Synthetic(body)
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => {
                rewrite_scoped_returns(body, depth, temp);
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                rewrite_scoped_returns(try_body, depth, temp);
                for clause in catches.iter_mut() {
                    rewrite_scoped_returns(&mut clause.body, depth, temp);
                }
                if let Some(body) = finally_body {
                    rewrite_scoped_returns(body, depth, temp);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => rewrite_scoped_returns(body, depth + 1, temp),
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases.iter_mut() {
                    rewrite_scoped_returns(body, depth + 1, temp);
                }
                if let Some(body) = default {
                    rewrite_scoped_returns(body, depth + 1, temp);
                }
            }
            _ => {}
        }
    }
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
                if confine_nested_returns(body, Some(temp)) {
                    // Every return is now an assignment plus `break`, and none of them is certain
                    // to run: report no capture, so the caller seeds the default `1` first.
                    return false;
                }
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
            let value = match original.kind {
                StmtKind::Return(Some(value)) => value,
                // A bare `return;` makes the include evaluate to NULL, not to the default `1`.
                // MEASURED: `$x = include F;` with F doing `return;` gives `NULL` in reference.
                _ => Expr::new(ExprKind::Null, span),
            };
            body[i] = assign_temp(temp, value, span);
            body.truncate(i + 1);
            return true;
        }
    }
    false
}
