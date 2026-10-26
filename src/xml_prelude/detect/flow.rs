//! Purpose:
//! Preserves receiver provenance across alternatives, loop back-edges, and dynamic DOM calls.
//!
//! Called from:
//! - The exhaustive XML statement and expression detector.
//!
//! Key details:
//! - Lexical assignments replace facts; only alternative paths join class sets.
//! - Loop input facts reach a finite fixed point before leaving the loop.

use crate::names::Name;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use super::Detector;
use std::cell::RefCell;
use std::rc::Rc;
use super::super::dom_receiver::DomReceiver;

type ExitFacts = Rc<RefCell<Option<DomReceiver>>>;

/// Destination states of one loop or switch, shared by its nested control-flow paths.
#[derive(Clone)]
pub(super) struct FlowFrame {
    breaks: ExitFacts,
    continues: Option<ExitFacts>,
}

/// Cleanup body crossed when a saved exit leaves its protected statement.
#[derive(Clone)]
pub(super) struct Finalizer { depth: usize, body: Rc<Vec<Stmt>> }

/// Recognizes a cleanup body which replaces every incoming completion.
fn replaces_completion(body: &[Stmt]) -> bool {
    body.last().is_some_and(|stmt| match &stmt.kind {
        StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Return(_) => true,
        StmtKind::ExprStmt(expr) => matches!(expr.kind, ExprKind::Throw(_)),
        StmtKind::If { then_body, elseif_clauses, else_body: Some(else_body), .. } =>
            replaces_completion(then_body) && replaces_completion(else_body)
                && elseif_clauses.iter().all(|(_, body)| replaces_completion(body)),
        _ => false,
    })
}

/// Accumulates one possible exit without adding an artificial entry path.
fn collect_exit(destination: &ExitFacts, receiver: &DomReceiver) {
    let mut destination = destination.borrow_mut();
    if let Some(previous) = destination.as_ref() { previous.join(receiver); }
    else { *destination = Some(receiver.clone()); }
}

impl Detector {
    /// Creates an independent control-flow path in the same lexical scope.
    pub(super) fn fork(&self) -> Self { Self { dom: self.dom.clone(), frames: self.frames.clone(), finalizers: self.finalizers.clone() } }

    /// Marks cleanup which an exit out of this try statement must execute.
    pub(super) fn push_finalizer(&mut self, body: Option<&[Stmt]>) {
        if let Some(body) = body.filter(|body| !body.is_empty()) {
            self.finalizers.push(Finalizer { depth: self.frames.len(), body: Rc::new(body.to_vec()) });
        }
    }

    /// Routes a numbered exit after all crossed finally bodies have updated its facts.
    pub(super) fn record_exit(&self, levels: usize, continuing: bool) -> bool {
        let Some(index) = self.frames.len().checked_sub(levels.max(1)) else { return false; };
        let mut path = self.fork();
        for (position, finalizer) in self.finalizers.iter().enumerate().rev() {
            if index >= finalizer.depth { continue; }
            path.finalizers.truncate(position);
            if path.body_refs_xml(&finalizer.body) { return true; }
            if replaces_completion(&finalizer.body) { return false; }
        }
        let frame = &self.frames[index];
        let destination = if continuing { frame.continues.as_ref().unwrap_or(&frame.breaks) } else { &frame.breaks };
        collect_exit(destination, &path.dom);
        false
    }

    /// Visits one sequential body without adding an artificial lexical scope.
    pub(super) fn body_refs_xml(&self, body: &[Stmt]) -> bool {
        body.iter().any(|stmt| self.stmt_refs_xml(stmt))
    }

    /// Joins both alternatives after evaluating a condition in the parent scope.
    pub(super) fn alternatives_refs_xml<'a>(&self, branches: impl IntoIterator<Item = &'a [Stmt]>) -> bool {
        let mut merged: Option<Self> = None;
        for body in branches {
            let path = self.fork();
            if path.body_refs_xml(body) { return true; }
            if let Some(merged) = &merged { merged.dom.join(&path.dom); }
            else { merged = Some(path); }
        }
        if let Some(merged) = merged { self.dom.replace(&merged.dom); }
        false
    }

    /// Carries false-path condition effects through elseif branches, then joins alternatives.
    pub(super) fn if_refs_xml(&self, condition: &Expr, then_body: &[Stmt], elseifs: &[(Expr, Vec<Stmt>)], else_body: Option<&[Stmt]>) -> bool {
        if self.expr_refs_xml(condition) { return true; }
        let remaining = self.fork();
        let merged = remaining.fork();
        if merged.body_refs_xml(then_body) { return true; }
        for (condition, body) in elseifs {
            if remaining.expr_refs_xml(condition) { return true; }
            let branch = remaining.fork();
            if branch.body_refs_xml(body) { return true; }
            merged.dom.join(&branch.dom);
        }
        if remaining.body_refs_xml(else_body.unwrap_or(&[])) { return true; }
        merged.dom.join(&remaining.dom);
        self.dom.replace(&merged.dom);
        false
    }

    /// Includes switch fallthrough while retaining direct entry to every case.
    pub(super) fn switch_refs_xml(&self, subject: &Expr, cases: &[(Vec<Expr>, Vec<Stmt>)], default: Option<&[Stmt]>) -> bool {
        if self.expr_refs_xml(subject) { return true; }
        let frame = FlowFrame { breaks: Rc::new(RefCell::new(None)), continues: None };
        let mut entry = self.fork();
        entry.frames.push(frame.clone());
        let merged = entry.fork();
        let fallthrough = entry.fork();
        for (conditions, body) in cases {
            let branch = entry.fork();
            branch.dom.join(&fallthrough.dom);
            if conditions.iter().any(|expr| branch.expr_refs_xml(expr)) || branch.body_refs_xml(body) { return true; }
            merged.dom.join(&branch.dom);
            fallthrough.dom.replace(&branch.dom);
        }
        if let Some(body) = default {
            let branch = entry.fork();
            branch.dom.join(&fallthrough.dom);
            if branch.body_refs_xml(body) { return true; }
            merged.dom.join(&branch.dom);
        }
        if let Some(exited) = frame.breaks.borrow().as_ref() { merged.dom.join(exited); }
        self.dom.replace(&merged.dom);
        false
    }

    /// Models the skipped argument path of a nullsafe call.
    pub(super) fn optional_args_refs_xml(&self, args: &[Expr]) -> bool {
        let called = self.fork();
        if args.iter().any(|expr| called.expr_refs_xml(expr)) { return true; }
        self.dom.join(&called.dom);
        false
    }

    /// Unions loop entry and back-edge states until another iteration adds no facts.
    pub(super) fn loop_refs_xml(
        &self, visit: impl Fn(&Self) -> bool, continuation: impl Fn(&Self) -> bool,
        exit_condition: Option<&Expr>, at_least_once: bool,
    ) -> bool {
        let frame = FlowFrame { breaks: Rc::new(RefCell::new(None)), continues: Some(Rc::new(RefCell::new(None))) };
        let mut header = self.fork();
        header.frames.push(frame.clone());
        if at_least_once && visit(&header) { return true; }
        loop {
            let iteration = header.fork();
            if visit(&iteration) { return true; }
            let mut changed = header.dom.join(&iteration.dom);
            let continued = frame.continues.as_ref().and_then(|facts| facts.borrow().clone());
            if let Some(continued) = continued {
                let path = header.fork();
                path.dom.replace(&continued);
                if continuation(&path) { return true; }
                changed |= header.dom.join(&path.dom);
            }
            if !changed { break; }
        }
        if exit_condition.is_some_and(|condition| header.expr_refs_xml(condition)) { return true; }
        if let Some(exited) = frame.breaks.borrow().as_ref() { header.dom.join(exited); }
        self.dom.replace(&header.dom);
        false
    }

    /// Joins conditional expression results without turning their writes into unconditional stores.
    pub(super) fn expression_alternatives_refs_xml(&self, expressions: &[Option<&Expr>]) -> bool {
        let mut merged: Option<Self> = None;
        for expression in expressions {
            let path = self.fork();
            if expression.is_some_and(|expr| path.expr_refs_xml(expr)) { return true; }
            if let Some(merged) = &merged { merged.dom.join(&path.dom); }
            else { merged = Some(path); }
        }
        if let Some(merged) = merged { self.dom.replace(&merged.dom); }
        false
    }

    /// Keeps match-arm assignments on alternative paths rather than applying every arm in sequence.
    pub(super) fn match_refs_xml(&self, subject: &Expr, arms: &[(Vec<Expr>, Expr)], default: Option<&Expr>) -> bool {
        if self.expr_refs_xml(subject) { return true; }
        let remaining = self.fork();
        let mut merged: Option<Self> = None;
        for (conditions, value) in arms {
            let input = remaining.fork();
            for condition in conditions {
                if remaining.expr_refs_xml(condition) { return true; }
                input.dom.join(&remaining.dom);
            }
            if input.expr_refs_xml(value) { return true; }
            if let Some(merged) = &merged { merged.dom.join(&input.dom); }
            else { merged = Some(input); }
        }
        if let Some(default) = default {
            if remaining.expr_refs_xml(default) { return true; }
            if let Some(merged) = &merged { merged.dom.join(&remaining.dom); }
            else { merged = Some(remaining); }
        }
        if let Some(merged) = merged { self.dom.replace(&merged.dom); }
        false
    }

    /// Treats a runtime method name as possibly loadXML only for a DOM-connected receiver.
    pub(super) fn dynamic_method_refs_xml(&self, object: &Expr, method: &Expr) -> bool {
        let possible = match &method.kind {
            ExprKind::StringLiteral(name) => name.eq_ignore_ascii_case("loadXML"),
            _ => true,
        };
        possible && self.dom.may_load_xml(object)
    }

    /// Detects parser-desugared and explicit callback-array calls using the shared argument planner.
    pub(super) fn callback_call_refs_xml(&self, name: &Name, args: &[Expr]) -> bool {
        if !["call_user_func", "call_user_func_array"].iter().any(|candidate| name.as_str().eq_ignore_ascii_case(candidate)) { return false; }
        let Some(sig) = crate::builtins::registry::function_sig(name.as_str()) else { return false; };
        let Some(span) = args.first().map(|arg| arg.span) else { return false; };
        let Ok(plan) = crate::types::call_args::plan_call_args(&sig, args, span, false, false) else { return false; };
        let args = plan.normalized_args();
        let Some(callback) = args.first() else { return false; };
        let ExprKind::ArrayLiteral(items) = &callback.kind else { return false; };
        items.len() == 2 && self.dynamic_method_refs_xml(&items[0], &items[1])
    }
}
