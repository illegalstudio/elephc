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
    /// The bound shared with the consumer loop.
    pub bound: Expr,
    /// Whether the fill loop used `<=` and therefore also wrote the bound's key.
    ///
    /// A consumer that switches from `<` to `<=` would otherwise read one key past the filled
    /// range through the non-null shortcut.
    pub inclusive: bool,
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
        let (counter, inclusive, bound) = loop_counter_and_bound(init?, condition?, update?)?;
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
            inclusive,
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
        let Some((counter, inclusive, bound)) = loop_counter_and_bound(init, condition, update)
        else {
            return false;
        };
        counter == fact.counter && inclusive == fact.inclusive && bound == &fact.bound
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
        let loop_counter_advance =
            self.checking_loop_update && stmt_is_counter_advance(stmt, &fact.counter);
        if (!resets_counter
            && !loop_counter_advance
            && (stmt_writes_local(stmt, &fact.counter)
                || stmt_expression_writes_inner(stmt, &fact.counter)))
            || bound_locals.iter().any(|name| {
                stmt_writes_local(stmt, name) || stmt_expression_writes(stmt, name)
            })
        {
            self.clear_fill_proof();
            return;
        }
        // A call in any expression this statement evaluates can mutate a filled array by
        // reference even when the assignment target is unrelated (`$removed = array_pop($x)`),
        // and an expression-level whole-array rebind (`$removed = ($x = [])`) replaces it.
        let removes = stmt_expressions_mention_call(stmt)
            || fact
                .arrays
                .iter()
                .any(|name| stmt_expression_writes(stmt, name))
            || match &stmt.kind {
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
            self.clear_fill_proof();
        }
    }

    /// Ends the fill proof and unmarks the reads active consumer loops already proved.
    ///
    /// A removing statement invalidates the whole loop, including reads checked before it in the
    /// body: the loop re-executes them after the removal. Only scopes still being checked are
    /// unmarked — a loop that already finished cannot re-execute its read, so a later call must
    /// not drop its raw slot access.
    fn clear_fill_proof(&mut self) {
        for scope in &self.active_fill_read_scopes {
            for site in scope {
                self.promotable_arrays.nonnull_read_sites.remove(site);
            }
        }
        self.filled_array_facts = None;
        self.nonnull_array_reads = None;
    }
}

/// Reports whether the statement is exactly the counter advance a `for` update performs.
///
/// Mirrors the advance forms `loop_counter_and_bound` accepts, so the loop's own `$i++` does not
/// clear the fill fact it is about to consume.
fn stmt_is_counter_advance(stmt: &Stmt, counter: &str) -> bool {
    match &stmt.kind {
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
    }
}

/// Visits every expression the statement itself evaluates, without entering nested bodies.
///
/// Conditions and `for` init/update expressions run inside the fill proof's scope and must be
/// inspected; nested statement bodies are checked one by one, so their expressions are not.
fn stmt_direct_expressions<'a>(
    stmt: &'a Stmt,
    include_for_control: bool,
    visitor: &mut dyn FnMut(&'a Expr),
) {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::PropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. }
        | StmtKind::PropertyArrayPush { value: expr, .. }
        | StmtKind::ArrayPush { value: expr, .. } => visitor(expr),
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ListUnpack { value, .. }
        | StmtKind::StaticVar { init: value, .. } => visitor(value),
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            visitor(index);
            visitor(value);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            visitor(target);
            visitor(value);
        }
        StmtKind::RefAssign { source, .. } => visitor(source),
        StmtKind::If {
            condition,
            elseif_clauses,
            ..
        } => {
            visitor(condition);
            for (condition, _) in elseif_clauses {
                visitor(condition);
            }
        }
        StmtKind::While { condition, .. } | StmtKind::DoWhile { condition, .. } => {
            visitor(condition);
        }
        StmtKind::For {
            init,
            condition,
            update,
            ..
        } => {
            // A loop's own init/update are its control writes. The counter check skips them:
            // every matching consumer increments its counter, and that must not invalidate the
            // fact before the loop it proves is even matched. The condition runs per iteration
            // and is always inspected.
            if include_for_control {
                if let Some(init) = init {
                    stmt_direct_expressions(init, include_for_control, visitor);
                }
            }
            if let Some(condition) = condition {
                visitor(condition);
            }
            if include_for_control {
                if let Some(update) = update {
                    stmt_direct_expressions(update, include_for_control, visitor);
                }
            }
        }
        StmtKind::Foreach { array, .. } => visitor(array),
        StmtKind::Switch { subject, cases, .. } => {
            visitor(subject);
            for (case_expressions, _) in cases {
                for expression in case_expressions {
                    visitor(expression);
                }
            }
        }
        StmtKind::Include { path, .. } => visitor(path),
        StmtKind::Return(Some(value)) => visitor(value),
        _ => {}
    }
}

/// Reports whether any expression the statement itself evaluates can run a call.
fn stmt_expressions_mention_call(stmt: &Stmt) -> bool {
    let mut found = false;
    stmt_direct_expressions(stmt, true, &mut |expr| {
        found = found || expr_mentions_call(expr);
    });
    found
}

/// Reports whether any expression the statement evaluates assigns, increments or decrements
/// the whole local `name`.
fn stmt_expression_writes(stmt: &Stmt, name: &str) -> bool {
    stmt_expression_writes_with(stmt, name, true)
}

/// Reports whether an expression the statement evaluates writes `name`, optionally skipping the
/// statement's own `for` init/update.
fn stmt_expression_writes_with(stmt: &Stmt, name: &str, include_for_control: bool) -> bool {
    let mut found = false;
    stmt_direct_expressions(stmt, include_for_control, &mut |expr| {
        found = found || expr_writes_local(expr, name);
    });
    found
}

/// Reports whether an expression the statement evaluates writes `name`, ignoring the statement's
/// own `for` init/update control writes.
fn stmt_expression_writes_inner(stmt: &Stmt, name: &str) -> bool {
    stmt_expression_writes_with(stmt, name, false)
}

/// Returns the counter, range operator and bound of `for ($i = 0; $i < B; $i++)`.
///
/// The boolean is `true` when the condition is inclusive (`<=`), which fills one more key.
fn loop_counter_and_bound<'a>(
    init: &'a Stmt,
    condition: &'a Expr,
    update: &'a Stmt,
) -> Option<(String, bool, &'a Expr)> {
    let StmtKind::Assign { name: counter, value } = &init.kind else {
        return None;
    };
    if !matches!(value.kind, ExprKind::IntLiteral(0)) {
        return None;
    }
    let (left, inclusive, right) = match &condition.kind {
        ExprKind::BinaryOp {
            left,
            op: BinOp::Lt,
            right,
        } => (left, false, right),
        ExprKind::BinaryOp {
            left,
            op: BinOp::LtEq,
            right,
        } => (left, true, right),
        _ => return None,
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
    advances.then_some((counter.clone(), inclusive, right))
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
    let direct = match &expr.kind {
        ExprKind::PreIncrement(local)
        | ExprKind::PostIncrement(local)
        | ExprKind::PreDecrement(local)
        | ExprKind::PostDecrement(local) => local == name,
        ExprKind::Assignment { target, .. } => {
            matches!(&target.kind, ExprKind::Variable(local) if local == name)
        }
        _ => false,
    };
    if direct {
        return true;
    }
    let mut found = false;
    super::loop_storage::visit_child_expressions(expr, &mut |child| {
        found = found || expr_writes_local(child, name);
    });
    found
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