//! Purpose:
//! Records arrays a preceding contiguous fill loop materialized, so a following loop with the
//! same counter and bound can treat element reads as non-null scalars.
//!
//! Called from:
//! - `crate::types::checker` statement control flow and array-read inference.
//!
//! Key details:
//! - A fill loop starts its counter at zero, advances by one, runs while it is below the same
//!   bound as the consumer, and writes every array element through that counter in its body.
//! - Any whole-array rebind, unset, reference binding or unknown statement clears the fact.
//!   Element writes and appends keep the already-filled prefix.
//! - The fact only proves keys `0..bound`; the consumer must read with the same counter, so a
//!   different index still takes the ordinary nullable path.

use std::collections::HashSet;

use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};

use super::Checker;

/// Arrays fully written by one contiguous fill loop.
#[derive(Clone)]
pub(crate) struct FilledArrayFact {
    /// The loop counter used as every write key.
    pub counter: String,
    /// The exclusive (or inclusive) bound shared with the consumer loop.
    pub bound: Expr,
    /// Local arrays the fill loop wrote.
    pub arrays: HashSet<String>,
}

/// Non-null element reads inside a loop that consumes a fill fact.
#[derive(Clone)]
pub(crate) struct NonnullArrayReads {
    /// The consumer loop's counter, matched against the read index.
    pub counter: String,
    /// Arrays the preceding fill loop materialized.
    pub arrays: HashSet<String>,
}

impl Checker {
    /// Returns the arrays a `for` loop fully fills, or `None` when it is not a pure fill loop.
    pub(crate) fn fill_loop_fact(
        init: Option<&Stmt>,
        condition: Option<&Expr>,
        update: Option<&Stmt>,
        body: &[Stmt],
    ) -> Option<FilledArrayFact> {
        let (counter, bound) = loop_counter_and_bound(init?, condition?, update?)?;
        let mut arrays = HashSet::new();
        for stmt in body {
            let StmtKind::ArrayAssign { array, index, .. } = &stmt.kind else {
                return None;
            };
            if !matches!(&index.kind, ExprKind::Variable(name) if name == &counter) {
                return None;
            }
            let StmtKind::ArrayAssign { value, .. } = &stmt.kind else {
                unreachable!("matched ArrayAssign above");
            };
            // The fill is only one-key-per-iteration if the stored value cannot advance the
            // counter, change the bound, or run code that mutates either through a reference.
            if expr_can_change_fill(value) || expr_can_change_fill(bound) {
                return None;
            }
            arrays.insert(array.clone());
        }
        (!arrays.is_empty()).then_some(FilledArrayFact {
            counter,
            bound: bound.clone(),
            arrays,
        })
    }

    /// Returns whether this loop consumes `fact`: same counter reset, bound and increment.
    pub(crate) fn loop_matches_fill_fact(
        init: Option<&Stmt>,
        condition: Option<&Expr>,
        update: Option<&Stmt>,
        fact: &FilledArrayFact,
    ) -> bool {
        let (Some(init), Some(condition), Some(update)) = (init, condition, update) else {
            return false;
        };
        let Some((counter, bound)) = loop_counter_and_bound(init, condition, update) else {
            return false;
        };
        counter == fact.counter && bound == &fact.bound
    }

    /// Returns whether `$name[$index]` is a non-null read under the active fill fact.
    pub(crate) fn nonnull_array_read(&self, name: &str, index: &Expr) -> bool {
        let Some(reads) = &self.nonnull_array_reads else {
            return false;
        };
        reads.arrays.contains(name)
            && matches!(&index.kind, ExprKind::Variable(counter) if counter == &reads.counter)
    }

    /// Records every element write of the fill loop as a runtime-promotable sparse site.
    ///
    /// The checker keeps the empty receiver packed; EIR uses the recorded site to open the raw
    /// scalar setter only for these writes, where the counter guarantees a contiguous key.
    pub(crate) fn record_fill_write_sites(&mut self, body: &[Stmt], counter: &str) {
        for stmt in body {
            let StmtKind::ArrayAssign { array, index, .. } = &stmt.kind else {
                continue;
            };
            if matches!(&index.kind, ExprKind::Variable(name) if name == counter) {
                self.sparse_key_write_sites
                    .entry(stmt.span)
                    .or_default()
                    .insert(array.clone());
            }
        }
    }

    /// Clears a fill fact when a statement can remove or replace a filled array.
    pub(crate) fn invalidate_filled_arrays(&mut self, stmt: &Stmt) {
        let Some(fact) = &mut self.filled_array_facts else {
            return;
        };
        // A write to the counter or to a bound local invalidates the key range the fact proves.
        let mut bound_locals = HashSet::new();
        expr_locals(&fact.bound, &mut bound_locals);
        // Re-entering the matching consumer resets the same counter to zero; that is the proof's
        // premise, not a write that invalidates it. Any other counter/bound write does.
        let resets_counter = matches!(
            &stmt.kind,
            StmtKind::Assign { name, value }
                if name == &fact.counter && matches!(value.kind, ExprKind::IntLiteral(0))
        );
        if (!resets_counter && stmt_writes_local(stmt, &fact.counter))
            || bound_locals.iter().any(|name| stmt_writes_local(stmt, name))
        {
            self.filled_array_facts = None;
            self.nonnull_array_reads = None;
            return;
        }
        let removes = match &stmt.kind {
            StmtKind::Assign { name, .. } | StmtKind::TypedAssign { name, .. } => {
                fact.arrays.contains(name)
            }
            // A reference binding can expose a filled array to later writes through the alias.
            StmtKind::RefAssign { .. } => true,
            StmtKind::Global { vars } => vars.iter().any(|name| fact.arrays.contains(name)),
            StmtKind::StaticVar { name, .. } => fact.arrays.contains(name),
            StmtKind::ExprStmt(expr) => expr_mentions_call(expr),
            StmtKind::Throw(_) | StmtKind::Include { .. } => true,
            _ => false,
        };
        if removes {
            // The active consumer loop must stop trusting the proof as soon as a statement can
            // remove or alias a filled array, not only the next loop.
            self.filled_array_facts = None;
            self.nonnull_array_reads = None;
        }
    }
}

/// Returns the counter and bound of `for ($i = 0; $i < B; $i++)`.
fn loop_counter_and_bound<'a>(
    init: &'a Stmt,
    condition: &'a Expr,
    update: &'a Stmt,
) -> Option<(String, &'a Expr)> {
    let StmtKind::Assign { name: counter, value } = &init.kind else {
        return None;
    };
    if !matches!(value.kind, ExprKind::IntLiteral(0)) {
        return None;
    }
    let ExprKind::BinaryOp {
        left,
        op: BinOp::Lt | BinOp::LtEq,
        right,
    } = &condition.kind
    else {
        return None;
    };
    if !matches!(&left.kind, ExprKind::Variable(name) if name == counter) {
        return None;
    }
    let advances = match &update.kind {
        StmtKind::ExprStmt(expr) => matches!(
            &expr.kind,
            ExprKind::PostIncrement(name) | ExprKind::PreIncrement(name) if name == counter
        ),
        StmtKind::Assign { name, value } => {
            name == counter
                && matches!(
                    &value.kind,
                    ExprKind::BinaryOp { left, op: BinOp::Add, right }
                        if matches!(&left.kind, ExprKind::Variable(name) if name == counter)
                            && matches!(right.kind, ExprKind::IntLiteral(1))
                )
        }
        _ => false,
    };
    advances.then_some((counter.clone(), right))
}

/// Collects the locals an expression reads.
fn expr_locals(expr: &Expr, out: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Variable(name) => {
            out.insert(name.clone());
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_locals(array, out);
            expr_locals(index, out);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_locals(left, out);
            expr_locals(right, out);
        }
        ExprKind::FunctionCall { args, .. } | ExprKind::NewObject { args, .. } => {
            for arg in args {
                expr_locals(arg, out);
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::InstanceOf { value: inner, .. } => expr_locals(inner, out),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_locals(condition, out);
            expr_locals(then_expr, out);
            expr_locals(else_expr, out);
        }
        _ => {}
    }
}

/// Reports whether a statement writes one local.
fn stmt_writes_local(stmt: &Stmt, name: &str) -> bool {
    match &stmt.kind {
        StmtKind::Assign { name: target, .. } | StmtKind::TypedAssign { name: target, .. } => {
            target == name
        }
        StmtKind::RefAssign { target, .. } => target == name,
        StmtKind::ArrayAssign { array, .. } | StmtKind::ArrayPush { array, .. } => array == name,
        StmtKind::Global { vars } => vars.iter().any(|var| var == name),
        StmtKind::StaticVar { name: target, .. } => target == name,
        StmtKind::ExprStmt(expr) => expr_writes_local(expr, name),
        _ => false,
    }
}

/// Reports whether an expression increments, decrements or assigns one local.
fn expr_writes_local(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::PreIncrement(local)
        | ExprKind::PostIncrement(local)
        | ExprKind::PreDecrement(local)
        | ExprKind::PostDecrement(local) => local == name,
        ExprKind::Assignment { target, .. } => {
            matches!(&target.kind, ExprKind::Variable(local) if local == name)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_writes_local(left, name) || expr_writes_local(right, name)
        }
        _ => false,
    }
}

/// Reports whether an expression can assign, increment, or call during the fill loop.
fn expr_can_change_fill(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Assignment { .. }
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::FunctionCall { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::ClosureCall { .. } => true,
        ExprKind::ArrayAccess { array, index } => {
            expr_can_change_fill(array) || expr_can_change_fill(index)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_can_change_fill(left) || expr_can_change_fill(right)
        }
        ExprKind::Negate(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::InstanceOf { value: inner, .. } => expr_can_change_fill(inner),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_can_change_fill(condition)
                || expr_can_change_fill(then_expr)
                || expr_can_change_fill(else_expr)
        }
        _ => false,
    }
}

/// Reports whether an expression can run a call (and therefore mutate a filled array by ref).
fn expr_mentions_call(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::FunctionCall { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::ClosureCall { .. } => true,
        ExprKind::ArrayAccess { array, index } => {
            expr_mentions_call(array) || expr_mentions_call(index)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_mentions_call(left) || expr_mentions_call(right)
        }
        ExprKind::Negate(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::InstanceOf { value: inner, .. } => expr_mentions_call(inner),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_mentions_call(condition)
                || expr_mentions_call(then_expr)
                || expr_mentions_call(else_expr)
        }
        _ => false,
    }
}
