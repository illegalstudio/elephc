//! Purpose:
//! Unrolls a `foreach` over a literal array whose body is a simple sequence of statements.
//!
//! Called from:
//! - `crate::optimize::control::fold::fold_block()`, before type checking.
//!
//! Key details:
//! - Each iteration gets an explicit assignment of the loop (and key) variable followed by a
//!   copy of the body whose reads of those variables are replaced by the literal. This is what
//!   lets `new ReflectionFunction($name)` and `new ReflectionMethod($class, $name)` reach the
//!   checker with the statically-known target names they require.
//! - A level-one `continue` becomes a per-iteration skip flag; bodies containing break, return,
//!   nested loops, declarations, or writes to the loop variables are left as loops. An empty
//!   literal unrolls to no statements, matching PHP's zero-iteration behavior.

use crate::parser::ast::{ArrayEntry, Expr, ExprKind, Stmt, StmtKind};

use super::fold::fold_stmt;

/// Returns the flattened statements for a literal `foreach`, or `None` when it must stay a loop.
pub(super) fn unroll(stmt: &Stmt) -> Option<Vec<Stmt>> {
    let StmtKind::Foreach {
        array,
        key_var,
        value_var,
        value_by_ref,
        body,
    } = &stmt.kind
    else {
        return None;
    };
    if *value_by_ref || !should_unroll(body, value_var, key_var.as_deref()) {
        return None;
    }
    let mut values = Vec::new();
    let mut next_index = 0i64;
    match &array.kind {
        ExprKind::ArrayLiteral(entries) => {
            for value in entries {
                let value = literal(value)?;
                values.push((Expr::int_lit(next_index), value));
                next_index += 1;
            }
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                let key = literal(key)?;
                if let ExprKind::IntLiteral(index) = key.kind {
                    next_index = next_index.max(index + 1);
                }
                values.push((key, literal(value)?));
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries {
                match entry {
                    ArrayEntry::Value(value) => {
                        let value = literal(value)?;
                        values.push((Expr::int_lit(next_index), value));
                        next_index += 1;
                    }
                    ArrayEntry::Keyed(key, value) => {
                        let key = literal(key)?;
                        if let ExprKind::IntLiteral(index) = key.kind {
                            next_index = next_index.max(index + 1);
                        }
                        values.push((key, literal(value)?));
                    }
                    ArrayEntry::Spread(_)
                    | ArrayEntry::Reference(_)
                    | ArrayEntry::KeyedReference(_, _) => return None,
                }
            }
        }
        _ => return None,
    }
    let continue_flag = continue_flag_name(stmt);
    let has_continue = body.iter().any(stmt_may_continue);
    let mut out = Vec::new();
    for (iteration, (key, value)) in values.into_iter().enumerate() {
        let mut iteration_stmts = Vec::new();
        iteration_stmts.push(loop_assignment(stmt, value_var, value.clone()));
        if let Some(key_var) = key_var {
            iteration_stmts.push(loop_assignment(stmt, key_var, key.clone()));
        }
        let substituted = crate::optimize::fold::expr::with_variable_substitution(
            value_var,
            &value,
            || {
                body.iter()
                    .cloned()
                    .map(|statement| match key_var {
                        Some(key_var) => crate::optimize::fold::expr::with_variable_substitution(
                            key_var,
                            &key,
                            || fold_stmt(statement),
                        ),
                        None => fold_stmt(statement),
                    })
                    .collect::<Vec<_>>()
            },
        );
        if has_continue {
            iteration_stmts.push(flag_assignment(&continue_flag, false, stmt));
            iteration_stmts.extend(transform_block(substituted, &continue_flag, stmt));
        } else {
            iteration_stmts.extend(substituted);
        }
        offset_iteration_spans(&mut iteration_stmts, iteration);
        out.extend(iteration_stmts);
    }
    Some(out)
}

/// Gives every statement copied for one iteration a distinct source position.
///
/// The checker keys local binding decisions by `(line, column)` because spans carry no file.
/// Repeating the same body at the same span would look like several incompatible bindings of
/// `$x`; the column offset keeps each unrolled copy distinct while retaining the source line.
fn offset_iteration_spans(stmts: &mut [Stmt], iteration: usize) {
    for stmt in stmts {
        offset_stmt_span(stmt, iteration);
    }
}

/// Applies the iteration column offset to one statement and its nested blocks.
fn offset_stmt_span(stmt: &mut Stmt, iteration: usize) {
    if iteration == 0 {
        return;
    }
    let offset = (iteration as u32).saturating_mul(1000);
    let span = stmt.span;
    let start = crate::span::Span::new_in_source(
        span.line,
        span.col.saturating_add(offset),
        span.source_id(),
    );
    let end = crate::span::Span::new_in_source(
        span.end_line,
        span.end_column().saturating_add(offset),
        span.source_id(),
    );
    stmt.span = crate::span::Span::with_end_from(start, end);
    match &mut stmt.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            offset_iteration_spans(then_body, iteration);
            for (_, body) in elseif_clauses {
                offset_iteration_spans(body, iteration);
            }
            if let Some(body) = else_body {
                offset_iteration_spans(body, iteration);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            offset_iteration_spans(try_body, iteration);
            for clause in catches {
                offset_iteration_spans(&mut clause.body, iteration);
            }
            if let Some(body) = finally_body {
                offset_iteration_spans(body, iteration);
            }
        }
        StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. } => {
            offset_iteration_spans(body, iteration);
        }
        _ => {}
    }
}

thread_local! {
    /// Distinguishes the skip flags of nested unrolled loops in one thread.
    static CONTINUE_FLAG_COUNTER: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Mints a compiler-generated local name for one unrolled loop's `continue` flag.
fn continue_flag_name(stmt: &Stmt) -> String {
    let index = CONTINUE_FLAG_COUNTER.with(|counter| {
        let next = counter.get();
        counter.set(next + 1);
        next
    });
    crate::names::generated_local_name(&format!(
        "unroll_continue_{}_{}_{}",
        stmt.span.line, stmt.span.col, index
    ))
}

/// Creates the per-iteration assignment of a loop variable, retaining the foreach's source mode.
fn loop_assignment(stmt: &Stmt, name: &str, value: Expr) -> Stmt {
    let mut assignment = Stmt::new(
        StmtKind::Assign {
            name: name.to_string(),
            value,
        },
        stmt.span,
    );
    assignment.source_mode = stmt.source_mode;
    assignment.strict_types = stmt.strict_types;
    assignment
}

/// Accepts only side-effect-free scalar literals as a loop element or key.
fn literal(expr: &Expr) -> Option<Expr> {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => Some(expr.clone()),
        _ => None,
    }
}

/// Returns whether this body is a simple reflection-constructor loop the unroller owns.
pub(super) fn should_unroll(body: &[Stmt], value_var: &str, key_var: Option<&str>) -> bool {
    body_is_unrollable(body, value_var, key_var)
        && crate::optimize::fold::expr::body_mentions_reflection_constructor(body, value_var)
}

/// Returns whether every statement in the loop body can be copied once per iteration.
fn body_is_unrollable(body: &[Stmt], value_var: &str, key_var: Option<&str>) -> bool {
    body.iter().all(|stmt| stmt_is_unrollable(stmt, value_var, key_var))
}

/// Returns whether one statement is a simple block or expression with no loop-variable write.
///
/// `continue` is allowed because the unroller turns it into a per-iteration skip flag. A
/// `try` is allowed only when it has no `finally`: a `continue` must still run a finally block,
/// which the plain statement sequence cannot model. Nested loops, break, return, switch and
/// declarations stay on the runtime iteration path.
fn stmt_is_unrollable(stmt: &Stmt, value_var: &str, key_var: Option<&str>) -> bool {
    match &stmt.kind {
        StmtKind::Echo(_) | StmtKind::ExprStmt(_) => true,
        StmtKind::Assign { name, .. } => Some(name.as_str()) != Some(value_var)
            && Some(name.as_str()) != key_var,
        StmtKind::Continue(1) => true,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            body_is_unrollable(then_body, value_var, key_var)
                && elseif_clauses
                    .iter()
                    .all(|(_, body)| body_is_unrollable(body, value_var, key_var))
                && else_body
                    .as_ref()
                    .is_none_or(|body| body_is_unrollable(body, value_var, key_var))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            finally_body.is_none()
                && body_is_unrollable(try_body, value_var, key_var)
                && catches
                    .iter()
                    .all(|clause| body_is_unrollable(&clause.body, value_var, key_var))
        }
        StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. } => {
            body_is_unrollable(body, value_var, key_var)
        }
        _ => false,
    }
}

/// Returns whether executing `stmt` can reach a `continue`.
fn stmt_may_continue(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Continue(_) => true,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_may_continue)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| body.iter().any(stmt_may_continue))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_may_continue))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_may_continue)
                || catches
                    .iter()
                    .any(|clause| clause.body.iter().any(stmt_may_continue))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_may_continue))
        }
        StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. } => {
            body.iter().any(stmt_may_continue)
        }
        _ => false,
    }
}

/// Rewrites a block so a `continue` sets `flag` and later statements skip when it is set.
fn transform_block(body: Vec<Stmt>, flag: &str, source: &Stmt) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut guarded = false;
    for statement in body {
        let may_continue = stmt_may_continue(&statement);
        let mut transformed = transform_stmt(statement, flag, source);
        if guarded {
            transformed = vec![guard_stmt(flag, transformed, source)];
        }
        out.extend(transformed);
        guarded |= may_continue;
    }
    out
}

/// Rewrites one statement for an unrolled iteration.
fn transform_stmt(stmt: Stmt, flag: &str, source: &Stmt) -> Vec<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::Continue(1) => vec![flag_assignment(flag, true, source)],
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => vec![Stmt::new(
            StmtKind::If {
                condition,
                then_body: transform_block(then_body, flag, source),
                elseif_clauses: elseif_clauses
                    .into_iter()
                    .map(|(condition, body)| {
                        (condition, transform_block(body, flag, source))
                    })
                    .collect(),
                else_body: else_body.map(|body| transform_block(body, flag, source)),
            },
            span,
        )],
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => vec![Stmt::new(
            StmtKind::Try {
                try_body: transform_block(try_body, flag, source),
                catches: catches
                    .into_iter()
                    .map(|mut clause| {
                        clause.body = transform_block(clause.body, flag, source);
                        clause
                    })
                    .collect(),
                finally_body,
            },
            span,
        )],
        StmtKind::Synthetic(body) => vec![Stmt::new(
            StmtKind::Synthetic(transform_block(body, flag, source)),
            span,
        )],
        StmtKind::NamespaceBlock { name, body } => vec![Stmt::new(
            StmtKind::NamespaceBlock {
                name,
                body: transform_block(body, flag, source),
            },
            span,
        )],
        kind => vec![Stmt::new(kind, span)],
    }
}

/// Builds `$flag = true|false;` with the loop's physical source profile.
fn flag_assignment(flag: &str, value: bool, source: &Stmt) -> Stmt {
    let mut assignment = Stmt::new(
        StmtKind::Assign {
            name: flag.to_string(),
            value: Expr::new(ExprKind::BoolLiteral(value), source.span),
        },
        source.span,
    );
    assignment.source_mode = source.source_mode;
    assignment.strict_types = source.strict_types;
    assignment
}

/// Wraps `body` in `if (!$flag) { … }` so a preceding `continue` skips it.
fn guard_stmt(flag: &str, body: Vec<Stmt>, source: &Stmt) -> Stmt {
    let mut guard = Stmt::new(
        StmtKind::If {
            condition: Expr::new(
                ExprKind::Not(Box::new(Expr::new(
                    ExprKind::Variable(flag.to_string()),
                    source.span,
                ))),
                source.span,
            ),
            then_body: body,
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        source.span,
    );
    guard.source_mode = source.source_mode;
    guard.strict_types = source.strict_types;
    guard
}

/// Returns whether `expr` is an array literal made only of side-effect-free scalar literals.
pub(super) fn is_safe_literal_array(expr: &Expr) -> bool {
    let literal_entry = |expr: &Expr| literal(expr).is_some();
    match &expr.kind {
        ExprKind::ArrayLiteral(values) => values.iter().all(literal_entry),
        ExprKind::ArrayLiteralAssoc(entries) => entries
            .iter()
            .all(|(key, value)| literal_entry(key) && literal_entry(value)),
        ExprKind::ArrayLiteralMixed(entries) => entries.iter().all(|entry| match entry {
            ArrayEntry::Value(value) | ArrayEntry::Reference(value) => literal_entry(value),
            ArrayEntry::Keyed(key, value) | ArrayEntry::KeyedReference(key, value) => {
                literal_entry(key) && literal_entry(value)
            }
            ArrayEntry::Spread(_) => false,
        }),
        _ => false,
    }
}
