//! Purpose:
//! Recognizes the one loop counter a still-empty array can be indexed by without leaving
//! packed storage: `for ($i = 0; …; $i++)`, written unconditionally in that loop's body.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::control_flow` (registers the counter for a `for` body)
//! - `crate::types::checker::stmt_check::assignments::arrays` (consults it at a write)
//!
//! Key details:
//! - Packed storage has no keys: slot `n` IS key `n`, so a write past the logical end zero-fills
//!   the gap, where php would have promoted the array to a hash. Every write this module cannot
//!   bound against the array's length takes hash storage instead.
//! - Both directions of an imperfect answer are safe. Accepting too much keeps the storage elephc
//!   already chose, so nothing gets worse than it is today; rejecting too much costs a hash where
//!   packed would have done, which is what php itself would have used.

use std::collections::HashSet;

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};

/// The `for` counter a still-empty array may be indexed by while staying on packed storage.
pub(crate) struct PackedLoopCounter {
    /// The counter local, which is `0` at the first write and advances one slot per iteration.
    pub name: String,
    /// The `local_conditional_depth` the loop body runs at. A write deeper than this sits under
    /// a branch that can skip it, and a skipped iteration is exactly how a gap appears.
    pub depth: u32,
    /// Locals the body REBINDS. The counter only tracks an array that keeps growing with it: a
    /// local reassigned in the body (`$a = [];`) starts over at length 0 while the counter runs
    /// on, so the very next write leaves a gap and the exception must not cover it.
    pub rebound_locals: HashSet<String>,
}

/// Returns the counter of a `for` loop whose `$a[$i] = …` writes may stay on packed storage.
///
/// The shape has to guarantee that the index is `0` at the first write into an empty array and
/// then grows one at a time with it, so three things are checked:
///
/// - the init is `$i = 0`, so the first iteration writes slot 0;
/// - the update is `$i++` or `++$i`, so each iteration advances by exactly one;
/// - the CONDITION does not write the counter, which `($i = 3) < 4` does before any write runs;
/// - the body neither `continue`s past a write nor assigns the counter again, either of which
///   would let the counter run ahead of the array's length.
///
/// A condition that only READS is still ignored on purpose: stopping the loop earlier ends the
/// array at a shorter length rather than leaving a hole in it.
pub(super) fn packed_for_counter(
    init: Option<&Stmt>,
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    body: &[Stmt],
    depth: u32,
) -> Option<PackedLoopCounter> {
    let name = counter_initialized_to_zero(init?)?;
    if counter_incremented_by_one(update?)? != name {
        return None;
    }
    if let Some(condition) = condition {
        if !expr_preserves_counter(condition, &name) {
            return None;
        }
    }
    if !body_preserves_counter(body, &name, 0) {
        return None;
    }
    let mut rebound_locals = HashSet::new();
    collect_rebound_locals(body, &mut rebound_locals);
    if let Some(condition) = condition {
        // The condition runs before every write, so a local it rebinds — the ARRAY, typically —
        // is as rebound as one the body assigns.
        collect_rebound_locals_from_expr(condition, &mut rebound_locals);
    }
    Some(PackedLoopCounter {
        name,
        depth,
        rebound_locals,
    })
}

/// Collects every local the loop body rebinds, at any depth, including nested loops.
fn collect_rebound_locals(body: &[Stmt], out: &mut HashSet<String>) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::Assign { name, .. }
            | StmtKind::TypedAssign { name, .. }
            | StmtKind::RefAssign { target: name, .. }
            | StmtKind::StaticVar { name, .. } => {
                out.insert(name.clone());
            }
            StmtKind::ListUnpack { vars, .. } | StmtKind::Global { vars } => {
                out.extend(vars.iter().cloned());
            }
            StmtKind::Foreach {
                key_var, value_var, ..
            } => {
                out.extend(key_var.iter().cloned());
                out.insert(value_var.clone());
            }
            _ => {}
        }
        for nested in nested_bodies(stmt) {
            collect_rebound_locals(nested, out);
        }
    }
}

/// Returns the statement lists nested inside one statement, excluding separate scopes.
fn nested_bodies(stmt: &Stmt) -> Vec<&[Stmt]> {
    match &stmt.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut bodies: Vec<&[Stmt]> = vec![then_body.as_slice()];
            bodies.extend(elseif_clauses.iter().map(|(_, clause)| clause.as_slice()));
            bodies.extend(else_body.as_deref().map(|stmts| stmts as &[Stmt]));
            bodies
        }
        StmtKind::Switch { cases, default, .. } => {
            let mut bodies: Vec<&[Stmt]> =
                cases.iter().map(|(_, case)| case.as_slice()).collect();
            bodies.extend(default.as_deref().map(|stmts| stmts as &[Stmt]));
            bodies
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let mut bodies: Vec<&[Stmt]> = vec![try_body.as_slice()];
            bodies.extend(catches.iter().map(|clause| clause.body.as_slice()));
            bodies.extend(finally_body.as_deref().map(|stmts| stmts as &[Stmt]));
            bodies
        }
        StmtKind::Synthetic(stmts) | StmtKind::IncludeOnceGuard { body: stmts, .. } => {
            vec![stmts.as_slice()]
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::For { body, .. } => vec![body.as_slice()],
        _ => Vec::new(),
    }
}

/// Returns the local a `for` init sets to literal `0`.
fn counter_initialized_to_zero(init: &Stmt) -> Option<String> {
    match &init.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            matches!(&value.kind, ExprKind::IntLiteral(0)).then(|| name.clone())
        }
        _ => None,
    }
}

/// Returns the local a `for` update advances by exactly one.
fn counter_incremented_by_one(update: &Stmt) -> Option<String> {
    let StmtKind::ExprStmt(expr) = &update.kind else {
        return None;
    };
    match &expr.kind {
        ExprKind::PostIncrement(name) | ExprKind::PreIncrement(name) => Some(name.clone()),
        _ => None,
    }
}

/// Returns true when nothing in `body` can make the counter skip a slot.
///
/// `loop_depth` counts the loops crossed since the counter's own loop, so that a `continue`
/// inside a NESTED loop is seen for what it is — a jump in that loop, which cannot skip a write
/// in ours — while `continue 2` out of it still counts. A nested function or class body is a
/// different scope entirely and is not descended into: neither its `continue` nor its `$i`
/// refers to this loop.
fn body_preserves_counter(body: &[Stmt], counter: &str, loop_depth: usize) -> bool {
    body
        .iter()
        .all(|stmt| stmt_preserves_counter(stmt, counter, loop_depth))
}

/// Returns true when no expression this statement carries writes the counter.
///
/// Nested statement bodies are NOT visited here — `stmt_preserves_counter` already recurses into
/// them with the loop depth they run at, which this has no way to track.
fn stmt_expressions_preserve_counter(stmt: &Stmt, counter: &str) -> bool {
    let preserved = |expr: &Expr| expr_preserves_counter(expr, counter);
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Return(Some(expr))
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::RefAssign { source: expr, .. }
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::Include { path: expr, .. }
        | StmtKind::While { condition: expr, .. }
        | StmtKind::DoWhile { condition: expr, .. }
        | StmtKind::Foreach { array: expr, .. }
        | StmtKind::Switch { subject: expr, .. }
        | StmtKind::PropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. }
        | StmtKind::PropertyArrayPush { value: expr, .. } => preserved(expr),
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::NestedArrayAssign {
            target: index,
            value,
        }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            preserved(index) && preserved(value)
        }
        StmtKind::If {
            condition,
            elseif_clauses,
            ..
        } => {
            preserved(condition)
                && elseif_clauses
                    .iter()
                    .all(|(condition, _)| preserved(condition))
        }
        // The `for` header's own expressions, minus the update: a counter this loop advances is
        // not a counter an ENCLOSING loop may trust, and `stmt_preserves_counter` rejects that
        // through the update's assignment target.
        StmtKind::For {
            condition: Some(condition),
            ..
        } => preserved(condition),
        _ => true,
    }
}

/// Returns true when one statement can neither skip a write nor retarget the counter.
///
/// Two independent checks. The structural one below reads each statement's own shape — how far a
/// `continue` jumps, which local an assignment binds. On top of it, every expression the statement
/// CARRIES is walked for a counter write, because the shape alone does not see one: `$z = $i++;`
/// binds `$z`, and the increment that matters is buried in its value.
fn stmt_preserves_counter(stmt: &Stmt, counter: &str, loop_depth: usize) -> bool {
    if !stmt_expressions_preserve_counter(stmt, counter) {
        return false;
    }
    match &stmt.kind {
        StmtKind::Continue(levels) => *levels <= loop_depth,
        StmtKind::Assign { name, .. }
        | StmtKind::TypedAssign { name, .. }
        | StmtKind::RefAssign { target: name, .. }
        | StmtKind::StaticVar { name, .. } => name != counter,
        StmtKind::ListUnpack { vars, .. } | StmtKind::Global { vars } => {
            !vars.iter().any(|var| var == counter)
        }
        StmtKind::ExprStmt(expr) => expr_preserves_counter(expr, counter),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            body_preserves_counter(then_body, counter, loop_depth)
                && elseif_clauses
                    .iter()
                    .all(|(_, clause)| body_preserves_counter(clause, counter, loop_depth))
                && else_body
                    .as_ref()
                    .is_none_or(|stmts| body_preserves_counter(stmts, counter, loop_depth))
        }
        StmtKind::Switch {
            cases, default, ..
        } => {
            cases
                .iter()
                .all(|(_, case)| body_preserves_counter(case, counter, loop_depth))
                && default
                    .as_ref()
                    .is_none_or(|stmts| body_preserves_counter(stmts, counter, loop_depth))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_preserves_counter(try_body, counter, loop_depth)
                && catches
                    .iter()
                    .all(|clause| body_preserves_counter(&clause.body, counter, loop_depth))
                && finally_body
                    .as_ref()
                    .is_none_or(|stmts| body_preserves_counter(stmts, counter, loop_depth))
        }
        StmtKind::Synthetic(stmts) | StmtKind::IncludeOnceGuard { body: stmts, .. } => {
            body_preserves_counter(stmts, counter, loop_depth)
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. } => {
            body_preserves_counter(body, counter, loop_depth + 1)
        }
        StmtKind::For {
            init, update, body, ..
        } => {
            // The condition is checked by the carrier walk above, with every other header piece
            // that is an expression.
            init.as_ref()
                .is_none_or(|stmt| stmt_preserves_counter(stmt, counter, loop_depth))
                && update
                    .as_ref()
                    .is_none_or(|stmt| stmt_preserves_counter(stmt, counter, loop_depth))
                && body_preserves_counter(body, counter, loop_depth + 1)
        }
        _ => true,
    }
}

/// Returns true when nothing ANYWHERE in this expression writes the counter.
///
/// The walk has to be deep: `f($i++)` and `($i = 3) < 4` both advance the counter from inside a
/// larger expression, and matching only the outermost node saw a `FunctionCall` and a `BinaryOp`.
/// Accepting one of those is not a harmless imprecision — it keeps packed storage for an index
/// that has already run past the array's length, which is the zero-filled gap php never has.
///
/// KNOWN HOLE, and the reason this is a `bool` and not a proof: a by-reference argument
/// (`bump($i)` where `bump(&$x)`) writes the counter with no assignment node to find. This pass
/// is syntactic and has no callee signature to consult, so such a loop keeps whatever storage it
/// gets today. Closure bodies are likewise not descended into, matching the shared walker.
fn expr_preserves_counter(expr: &Expr, counter: &str) -> bool {
    if assignment_target_name(expr).is_some_and(|name| name == counter) {
        return false;
    }
    let mut preserved = true;
    super::loop_storage::visit_child_expressions(expr, &mut |child| {
        preserved = preserved && expr_preserves_counter(child, counter);
    });
    preserved
}

/// Returns the local one expression writes directly, if it writes one.
fn assignment_target_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::PostIncrement(name)
        | ExprKind::PreIncrement(name)
        | ExprKind::PostDecrement(name)
        | ExprKind::PreDecrement(name) => Some(name.as_str()),
        ExprKind::Assignment { target, .. } => match &target.kind {
            ExprKind::Variable(name) => Some(name.as_str()),
            _ => None,
        },
        _ => None,
    }
}

/// Collects every local an expression rebinds, at any depth inside it.
fn collect_rebound_locals_from_expr(expr: &Expr, out: &mut HashSet<String>) {
    if let Some(name) = assignment_target_name(expr) {
        out.insert(name.to_string());
    }
    super::loop_storage::visit_child_expressions(expr, &mut |child| {
        collect_rebound_locals_from_expr(child, out)
    });
}
