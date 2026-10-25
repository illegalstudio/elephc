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
use crate::parser::ast::{Expr, ExprKind, Stmt};
use super::Detector;

impl Detector {
    /// Creates an independent control-flow path in the same lexical scope.
    pub(super) fn fork(&self) -> Self { Self { dom: self.dom.clone() } }

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
        let entry = self.fork();
        let merged = self.fork();
        let fallthrough = self.fork();
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
    pub(super) fn loop_refs_xml(&self, visit: impl Fn(&Self) -> bool) -> bool {
        let header = self.fork();
        loop {
            let iteration = header.fork();
            if visit(&iteration) { return true; }
            if !header.dom.join(&iteration.dom) { break; }
        }
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
