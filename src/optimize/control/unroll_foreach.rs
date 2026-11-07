//! Purpose:
//! Unrolls a `foreach` over a literal array whose body is a simple, control-transfer-free
//! sequence of statements.
//!
//! Called from:
//! - `crate::optimize::control::fold::fold_block()`, before type checking.
//!
//! Key details:
//! - Each iteration gets an explicit assignment of the loop (and key) variable followed by a
//!   copy of the body whose reads of those variables are replaced by the literal. This is what
//!   lets `new ReflectionFunction($name)` and `new ReflectionMethod($class, $name)` reach the
//!   checker with the statically-known target names they require.
//! - Bodies containing break/continue/return, nested loops, declarations, or writes to the loop
//!   variables are left as loops. An empty literal unrolls to no statements, matching PHP's
//!   zero-iteration behavior.

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
    let mut out = Vec::new();
    for (key, value) in values {
        out.push(loop_assignment(stmt, value_var, value.clone()));
        if let Some(key_var) = key_var {
            out.push(loop_assignment(stmt, key_var, key.clone()));
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
        out.extend(substituted);
    }
    Some(out)
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
fn stmt_is_unrollable(stmt: &Stmt, value_var: &str, key_var: Option<&str>) -> bool {
    match &stmt.kind {
        StmtKind::Echo(_) | StmtKind::ExprStmt(_) => true,
        StmtKind::Assign { name, .. } => Some(name.as_str()) != Some(value_var)
            && Some(name.as_str()) != key_var,
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
        StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. } => {
            body_is_unrollable(body, value_var, key_var)
        }
        _ => false,
    }
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
