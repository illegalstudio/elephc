//! Purpose:
//! Selects writes that can reach another iteration of a loop.
//!
//! Called from:
//! - Loop storage stabilization before checking loop-entry read views.
//!
//! Key details:
//! - Backward reachability distinguishes returns and breaks from continuing paths.
//! - Nested targets count switches, and finally blocks can override pending jumps.

use crate::parser::ast::{Expr, Stmt, StmtKind};

use super::loop_storage::{collect_value_assignment_stmt, collect_value_assignments_from_expr, LoopWrites};

/// Collects writes on possible back edges, including a trailing update or condition.
pub(super) fn backedge_writes<'a>(
    body: &'a [Stmt], update: Option<&'a Stmt>, condition: Option<&'a Expr>,
    diverges: &dyn Fn(&Expr) -> bool,
) -> LoopWrites<'a> {
    let mut scan = Backedges { writes: LoopWrites::default(), diverges };
    if let Some(expr) = condition { scan.expr(expr, true); }
    let tail = update.map_or(true, |stmt| scan.stmt(stmt, true, &[]));
    if scan.block(body, tail, &[(false, tail)]) { scan.writes }
    else { LoopWrites::default() }
}

/// Backward reachability and accumulated write evidence for one source walk.
struct Backedges<'a, 'b> {
    writes: LoopWrites<'a>,
    diverges: &'b dyn Fn(&Expr) -> bool,
}

impl<'a> Backedges<'a, '_> {
    /// Records expression effects only when execution can still reach a back edge.
    fn expr(&mut self, expr: &'a Expr, live: bool) {
        if live { collect_value_assignments_from_expr(expr, &mut self.writes); }
    }

    /// Walks a sequence backwards so terminal statements discard unreachable suffixes.
    fn block(&mut self, body: &'a [Stmt], mut live: bool, targets: &[(bool, bool)]) -> bool {
        for stmt in body.iter().rev() { live = self.stmt(stmt, live, targets); }
        live
    }

    /// Joins alternatives and follows PHP's numbered break/continue destinations.
    fn stmt(&mut self, stmt: &'a Stmt, after: bool, targets: &[(bool, bool)]) -> bool {
        match &stmt.kind {
            StmtKind::Break(levels) | StmtKind::Continue(levels) => {
                targets.len().checked_sub(*levels).map(|index| {
                    if matches!(stmt.kind, StmtKind::Break(_)) { targets[index].0 }
                    else { targets[index].1 }
                }).unwrap_or(false)
            }
            StmtKind::Return(_) | StmtKind::Throw(_) => false,
            StmtKind::ExprStmt(expr) if (self.diverges)(expr) => false,
            StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
                let mut live = else_body.as_ref().map_or(after, |body| self.block(body, after, targets));
                for (condition, body) in elseif_clauses.iter().rev() {
                    live |= self.block(body, after, targets);
                    self.expr(condition, live);
                }
                live |= self.block(then_body, after, targets);
                self.expr(condition, live);
                live
            }
            StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. } => self.block(body, after, targets),
            StmtKind::IncludeOnceGuard { body, .. } => after | self.block(body, after, targets),
            StmtKind::IfDef { then_body, else_body, .. } => {
                self.block(then_body, after, targets)
                    | else_body.as_ref().map_or(after, |body| self.block(body, after, targets))
            }
            StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
                let live = self.nested_loop(body, None, after, targets);
                self.expr(condition, live);
                live
            }
            StmtKind::For { init, condition, update, body } => {
                let live = self.nested_loop(body, update.as_deref(), after, targets);
                if let Some(condition) = condition { self.expr(condition, live); }
                init.as_deref().map_or(live, |stmt| self.stmt(stmt, live, targets))
            }
            StmtKind::Foreach { array, body, key_var, value_var, .. } => {
                let live = self.nested_loop(body, None, after, targets);
                self.expr(array, live);
                if live {
                    use super::loop_storage::AssignedValue;
                    self.writes.values.push((value_var, AssignedValue::Opaque));
                    if let Some(key) = key_var { self.writes.values.push((key, AssignedValue::Opaque)); }
                }
                live
            }
            StmtKind::Switch { subject, cases, default } => {
                let mut nested = targets.to_vec();
                nested.push((after, after));
                let mut arms: Vec<(&[Expr], &[Stmt])> = cases.iter()
                    .map(|(values, body)| (values.as_slice(), body.as_slice())).collect();
                if let Some(body) = default {
                    let position = crate::termination::switch_default_position(cases, body);
                    arms.insert(position, (&[], body));
                }
                let mut fallthrough = after;
                let mut entry = default.is_none() && after;
                for (values, body) in arms.into_iter().rev() {
                    fallthrough = self.block(body, fallthrough, &nested);
                    entry |= fallthrough;
                    for value in values { self.expr(value, entry); }
                }
                self.expr(subject, entry);
                entry
            }
            StmtKind::Try { try_body, catches, finally_body } => {
                let mut nested = targets.to_vec();
                let mut live = after;
                if let Some(body) = finally_body {
                    live = self.block(body, after, targets);
                    for (index, (breaking, continuing)) in targets.iter().copied().enumerate() {
                        nested[index] = (self.block(body, breaking, targets), self.block(body, continuing, targets));
                    }
                }
                let mut entry = self.block(try_body, live, &nested);
                for catch in catches { entry |= self.block(&catch.body, live, &nested); }
                // Any throwing operation in the try may reach a catch, including one before
                // a terminal statement. Keep its write evidence when a catch can continue.
                if !catches.is_empty() && entry {
                    for stmt in try_body { collect_value_assignment_stmt(stmt, &mut self.writes); }
                }
                entry
            }
            _ => {
                if after { collect_value_assignment_stmt(stmt, &mut self.writes); }
                after
            }
        }
    }

    /// Iterates the boolean loop-header reachability lattice for a nested loop.
    fn nested_loop(&mut self, body: &'a [Stmt], update: Option<&'a Stmt>, after: bool, targets: &[(bool, bool)]) -> bool {
        let mut live = after;
        loop {
            let tail = update.map_or(live, |stmt| self.stmt(stmt, live, targets));
            let mut nested = targets.to_vec();
            nested.push((after, tail));
            let entry = after | self.block(body, tail, &nested);
            if entry == live { return entry; }
            live = entry;
        }
    }
}
