//! Purpose:
//! Retains function-exit states until the finally blocks on those paths are checked.
//!
//! Called from:
//! - Try/catch/finally, return statements, and explicit throw inference.
//!
//! Key details:
//! - Catch handlers intercept matching throws before an outer finally runs.
//! - Function-like scopes isolate the stack, and a diverging finally replaces pending exits.

use crate::types::{PhpType, TypeEnv};
use crate::optimize::CheckerThrownTypes;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::errors::CompileError;
use crate::span::Span;

use super::inference::FlowSnapshot;
use super::Checker;

/// Explicit function exits and catch boundaries for one active try statement.
pub(super) struct FinallyFlowFrame {
    catch_types: Vec<Vec<String>>,
    catch_depth: u32,
    checking_catches: bool,
    has_finally: bool,
    inputs: Vec<FinallyEdge>,
}

/// One unwind state, intercepted by a handler or continuing through this finally.
struct FinallyEdge {
    handler: Option<usize>,
    flow: FlowSnapshot,
}

impl Checker {
    /// Records source-point assignment types without rechecking finally under a narrower input.
    pub(super) fn record_finally_assignment_type(&mut self, value: &Expr, ty: &PhpType) {
        if self.finally_break_continue_bases.is_empty() { return; }
        let key = (self.current_loop_storage_scope.clone(), value as *const Expr as usize);
        let observed = self.finally_assigned_value_types.get(&key)
            .filter(|(span, _)| *span == value.span)
            .map_or_else(|| ty.clone(), |(_, previous)| self.join_flow_views(previous, ty));
        self.finally_assigned_value_types.insert(key, (value.span, observed));
    }

    /// Shares throw validation between statement, expression, and effect inference.
    pub(super) fn validate_finally_throw_type(&self, thrown: &PhpType, span: Span) -> Result<(), CompileError> {
        match thrown {
            PhpType::Object(name) if self.object_type_implements_throwable(name)
                || self.object_type_may_be_eval_declared(name) => Ok(()),
            PhpType::Mixed if self.eval_barrier_active => Ok(()),
            PhpType::Object(_) => Err(CompileError::new(span,
                "Type error: throw requires an object implementing Throwable")),
            _ => Err(CompileError::new(span, "Type error: throw requires an object value")),
        }
    }
    /// Opens a catch boundary even when it has no finally of its own.
    pub(super) fn begin_finally_region(&mut self, catch_types: Vec<Vec<String>>, has_finally: bool) {
        self.finally_flow_frames.push(FinallyFlowFrame {
            catch_types, catch_depth: self.local_conditional_depth,
            checking_catches: false, has_finally, inputs: Vec::new(),
        });
    }

    /// Marks handlers so an exception in a catch cannot be intercepted by a sibling catch.
    pub(super) fn enter_finally_catches(&mut self) {
        self.finally_flow_frames.last_mut().expect("active try").checking_catches = true;
    }

    /// Closes the try before checking its finally, whose exits belong to outer regions.
    pub(super) fn end_finally_region(&mut self) -> Vec<FlowSnapshot> {
        self.finally_flow_frames.pop().expect("active try").inputs.into_iter()
            .filter(|edge| edge.handler.is_none()).map(|edge| edge.flow).collect()
    }

    /// Records validation before a return unwinds through its local catches and finally blocks.
    pub(super) fn record_return_boundary_exception_flow(&mut self, source: &PhpType, env: &TypeEnv) {
        if self.finally_flow_frames.is_empty()
            || !crate::optimize::active_return_boundary_checks_source(source) { return; }
        self.record_exception_flow(CheckerThrownTypes::from_value(&PhpType::Object("TypeError".into()), true), env);
    }

    /// Saves a return before an enclosing guard restores its fallthrough view.
    pub(super) fn record_finally_return(&mut self, env: &TypeEnv) {
        if !self.finally_flow_frames.iter().any(|frame| frame.has_finally) { return; }
        let edge = self.snapshot_flow_exit(env);
        for frame in &mut self.finally_flow_frames {
            if frame.has_finally { frame.inputs.push(FinallyEdge { handler: None, flow: edge.clone() }); }
        }
    }

    /// Restores each handler from its own first-match incoming states.
    pub(super) fn enter_catch_flow(&mut self, handler: usize, fallback: &FlowSnapshot, env: &mut TypeEnv) -> bool {
        let edges: Vec<_> = self.finally_flow_frames.last().expect("active try").inputs.iter()
            .filter(|edge| edge.handler == Some(handler)).map(|edge| edge.flow.clone()).collect();
        let reachable = !edges.is_empty();
        let incoming = self.join_flow_snapshots(&edges).unwrap_or_else(|| fallback.clone());
        self.restore_flow_snapshot(&incoming, env);
        reachable
    }

    /// Allows a direct catch-body fallback store because handlers have independent flow joins.
    pub(super) fn coalescing_flow_scope(&self) -> bool {
        self.local_conditional_depth == 0 || self.finally_flow_frames.last().is_some_and(|frame|
            frame.checking_catches && frame.catch_depth == self.local_conditional_depth)
    }

    /// Saves explicit throws for their matching handler and each finally crossed beforehand.
    pub(super) fn record_finally_throw(&mut self, thrown: &PhpType, value: &Expr, env: &TypeEnv) {
        self.record_exception_flow(CheckerThrownTypes::from_value(thrown, matches!(value.kind, ExprKind::NewObject { .. })), env);
    }

    /// Records implicit exceptions at operation boundaries after source-order operand effects.
    pub(super) fn record_operation_exception_flow(&mut self, expr: &Expr, env: &TypeEnv) {
        if self.finally_flow_frames.is_empty() || matches!(expr.kind, ExprKind::Throw(_)) { return; }
        let Some(analysis) = &self.exception_analysis else { return; };
        let thrown = analysis.operation_throws(expr, self.current_class.as_deref());
        self.record_exception_flow(thrown, env);
    }

    /// Records statement callbacks after operand effects and post-store cleanup.
    pub(super) fn record_statement_exception_flow(&mut self, stmt: &Stmt, env: &TypeEnv, retired_type: Option<&PhpType>, before_write: Option<FlowSnapshot>) {
        if self.finally_flow_frames.is_empty() { return; }
        // The call and its operands already contributed their operation edges. A declared
        // never call produces no statement result whose destruction could throw afterwards.
        if matches!(&stmt.kind, StmtKind::ExprStmt(expr) if self.expr_is_declared_never_call(expr)) {
            return;
        }
        let Some(analysis) = &self.exception_analysis else { return; };
        let thrown = analysis.statement_throws(stmt, self.current_class.as_deref(), retired_type);
        if thrown.is_empty() { return; }
        let after = self.snapshot_flow_exit(env);
        // A container/property store can throw before replacing its destination. Keep that
        // state as well as operand-side writes observed by the successful checker walk.
        let edge = before_write.and_then(|before| self.join_flow_snapshots(&[before, after.clone()])).unwrap_or(after);
        self.record_exception_snapshot(thrown, edge);
    }

    /// Records iterator callbacks with joined normal and continue back-edge states.
    pub(super) fn record_iteration_exception_flow(&mut self, stmt: &Stmt) {
        if self.finally_flow_frames.is_empty() { return; }
        let Some(edge) = self.loop_backedge_snapshot() else { return; };
        let Some(analysis) = &self.exception_analysis else { return; };
        let thrown = analysis.statement_throws(stmt, self.current_class.as_deref(), None);
        self.record_exception_snapshot(thrown, edge);
    }

    /// Preserves unknown throwable domains across source-order handlers and nested regions.
    fn record_exception_flow(&mut self, thrown: CheckerThrownTypes, env: &TypeEnv) {
        if thrown.is_empty() || self.finally_flow_frames.is_empty() { return; }
        let edge = self.snapshot_flow_exit(env);
        self.record_exception_snapshot(thrown, edge);
    }

    /// Routes an already captured operation-point snapshot without changing current reads.
    fn record_exception_snapshot(&mut self, mut thrown: CheckerThrownTypes, edge: FlowSnapshot) {
        if thrown.is_empty() { return; }
        let Some(analysis) = &self.exception_analysis else { return; };
        for frame in self.finally_flow_frames.iter_mut().rev() {
            if !frame.checking_catches {
                for (handler, types) in frame.catch_types.iter().enumerate() {
                    let (caught, remaining) = analysis.split(thrown, types);
                    if !caught.is_empty() {
                        frame.inputs.push(FinallyEdge { handler: Some(handler), flow: edge.clone() });
                    }
                    thrown = remaining;
                }
            }
            if thrown.is_empty() { break; }
            if frame.has_finally {
                frame.inputs.push(FinallyEdge { handler: None, flow: edge.clone() });
            }
        }
    }

    /// Marks outer exits before checking a nested try or its finally.
    pub(super) fn pending_finally_exit_counts(&self) -> Vec<usize> {
        self.finally_flow_frames.iter().map(|frame| frame.inputs.len()).collect()
    }

    /// Removes synthetic exits discovered while type-checking an unreachable suffix.
    pub(super) fn discard_finally_exits_after(&mut self, counts: &[usize]) {
        for (frame, count) in self.finally_flow_frames.iter_mut().zip(counts) {
            frame.inputs.truncate(*count);
        }
    }

    /// Applies finally effects to pending exits while preserving new exits from the finally.
    pub(super) fn apply_finally_to_function_exits(
        &mut self, before_try: &[usize], before_finally: &[usize], env: &TypeEnv, diverges: bool,
    ) {
        let after = self.snapshot_flow_exit(env);
        for (index, start) in before_try.iter().copied().enumerate() {
            if index >= self.finally_flow_frames.len() { break; }
            let split = start.min(self.finally_flow_frames[index].inputs.len());
            let mut pending = self.finally_flow_frames[index].inputs.split_off(split);
            let boundary = before_finally.get(index).copied().unwrap_or(split).saturating_sub(split);
            let from_finally = pending.split_off(boundary.min(pending.len()));
            if !diverges {
                for edge in &mut pending { self.join_finally_flow_snapshot(&mut edge.flow, &after); }
                self.finally_flow_frames[index].inputs.extend(pending);
            }
            self.finally_flow_frames[index].inputs.extend(from_finally);
        }
    }
}
