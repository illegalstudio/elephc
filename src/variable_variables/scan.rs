//! Purpose:
//! Cheap read-only pre-scan deciding whether a body needs the variable-variable rewrite.
//!
//! Called from:
//! - `super::rewrite::Rewriter` before it collects a frame's names.
//!
//! Key details:
//! - The scan descends into nested functions, methods and closures, so a frame is entered
//!   whenever anything inside it needs rewriting; the rewrite then re-checks each nested frame.

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};

use super::rewrite::builtin_call_name;
use super::walk::{walk_expr_children, walk_stmt_children, Visitor};
use super::FRAME_LOCAL;

/// Returns whether `body` holds a variable variable or a `compact()`/`extract()` call.
pub(super) fn needs_rewrite(body: &mut Vec<Stmt>) -> bool {
    let mut scan = Scan { found: false };
    scan.block(body);
    scan.found
}

struct Scan {
    found: bool,
}

impl Visitor for Scan {
    fn stmt(&mut self, stmt: &mut Stmt) {
        if self.found {
            return;
        }
        match &mut stmt.kind {
            // Destructuring into `$$x` lowers to a store whose array is named by string.
            StmtKind::ArrayAssign { array, .. } | StmtKind::ArrayPush { array, .. }
                if array == FRAME_LOCAL =>
            {
                self.found = true
            }
            StmtKind::FunctionDecl { body, .. } => self.block(body),
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::InterfaceDecl { methods, .. }
            | StmtKind::EnumDecl { methods, .. } => {
                for method in methods.iter_mut() {
                    self.block(&mut method.body);
                }
            }
            _ => walk_stmt_children(self, stmt),
        }
    }

    fn expr(&mut self, expr: &mut Expr) {
        if self.found {
            return;
        }
        match &mut expr.kind {
            ExprKind::Variable(name) if name == FRAME_LOCAL => self.found = true,
            ExprKind::FunctionCall { name, .. }
                if matches!(builtin_call_name(name).as_deref(), Some("compact" | "extract")) =>
            {
                self.found = true
            }
            ExprKind::Closure { body, .. } => self.block(body),
            _ => walk_expr_children(self, expr),
        }
    }
}

/// Returns the span of every frame access that survived the desugar, in expression position or
/// as a statement's variable name. Any hit is reported as an explicit compile error so the
/// internal pseudo-variable can never reach the checker (or a diagnostic).
pub(super) fn leftover_frame_accesses(program: &mut Vec<Stmt>) -> Vec<crate::span::Span> {
    let mut check = Leftover { spans: Vec::new() };
    check.block(program);
    check.spans
}

struct Leftover {
    spans: Vec<crate::span::Span>,
}

impl Visitor for Leftover {
    fn stmt(&mut self, stmt: &mut Stmt) {
        let span = stmt.span;
        let names: Vec<&String> = match &stmt.kind {
            StmtKind::Assign { name, .. }
            | StmtKind::TypedAssign { name, .. }
            | StmtKind::RefAssign { target: name, .. }
            | StmtKind::ArrayAssign { array: name, .. }
            | StmtKind::ArrayPush { array: name, .. }
            | StmtKind::StaticVar { name, .. } => vec![name],
            StmtKind::ListUnpack { vars, .. } | StmtKind::Global { vars } => vars.iter().collect(),
            StmtKind::Foreach {
                key_var, value_var, ..
            } => std::iter::once(value_var).chain(key_var.iter()).collect(),
            _ => Vec::new(),
        };
        if names.iter().any(|name| name.as_str() == FRAME_LOCAL) {
            self.spans.push(span);
        }
        match &mut stmt.kind {
            StmtKind::FunctionDecl { body, .. } => self.block(body),
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::InterfaceDecl { methods, .. }
            | StmtKind::EnumDecl { methods, .. } => {
                for method in methods.iter_mut() {
                    self.block(&mut method.body);
                }
            }
            _ => walk_stmt_children(self, stmt),
        }
    }

    fn expr(&mut self, expr: &mut Expr) {
        let span = expr.span;
        match &mut expr.kind {
            ExprKind::Variable(name)
            | ExprKind::PreIncrement(name)
            | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name)
            | ExprKind::PostDecrement(name)
                if name == FRAME_LOCAL =>
            {
                self.spans.push(span)
            }
            ExprKind::Closure {
                body,
                captures,
                capture_refs,
                ..
            } => {
                if captures.iter().chain(capture_refs.iter()).any(|c| c == FRAME_LOCAL) {
                    self.spans.push(span);
                }
                self.block(body);
            }
            _ => walk_expr_children(self, expr),
        }
    }
}
