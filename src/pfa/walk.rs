//! Purpose:
//! Walks the program and replaces every marked call with its partial-application closure.
//!
//! Called from:
//! - `crate::pfa::desugar`.
//!
//! Key details:
//! - The reusable magic-constant walker visits children before parents and reports lexical
//!   scope entry/exit, so nested partial applications and `$this`/`static::` keep their
//!   enclosing meaning.
//! - A small best-effort local map resolves `$obj = new C; $obj->m(?)` method signatures;
//!   when it cannot, the closure is still built with untyped parameters.
//! - Errors are collected and reported in source walk order; a failed rewrite keeps the
//!   original call so the walk can continue and surface the first diagnostic.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::errors::CompileError;
use crate::magic_constants::walker::Pass;
use crate::parser::ast::{Expr, ExprKind, MagicConstant, Stmt, StmtKind};
use crate::span::Span;

use super::lower::{self, args_contain_marker, Callee, LowerContext};
use super::signatures::Signatures;

/// One rewriting pass over a program.
pub(super) struct Desugar {
    signatures: Signatures,
    enabled: bool,
    current_class: Option<String>,
    class_stack: Vec<Option<String>>,
    locals: RefCell<HashMap<String, String>>,
    local_stack: RefCell<Vec<HashMap<String, String>>>,
    errors: RefCell<Vec<CompileError>>,
}

impl Desugar {
    /// Creates a pass over declarations already collected in `signatures`.
    pub(super) fn new(signatures: Signatures, enabled: bool) -> Self {
        Self {
            signatures,
            enabled,
            current_class: None,
            class_stack: Vec::new(),
            locals: RefCell::new(HashMap::new()),
            local_stack: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
        }
    }

    /// Consumes the pass and returns the first collected diagnostic.
    pub(super) fn into_error(self) -> Option<CompileError> {
        self.errors.into_inner().into_iter().next()
    }

    /// Records one diagnostic and leaves the original expression in place.
    fn fail(&self, error: CompileError, expr: Expr) -> Expr {
        self.errors.borrow_mut().push(error);
        expr
    }

    /// Rewrites a marked call, or returns the expression unchanged when it has no marker.
    fn lower_call(&self, expr: Expr, callee: Callee, args: &[Expr]) -> Expr {
        if !args_contain_marker(args) {
            return expr;
        }
        if !self.enabled {
            return self.fail(
                CompileError::new(
                    expr.span,
                    "Partial function application requires --php-version=8.6",
                ),
                expr,
            );
        }
        let locals = self.locals.borrow();
        let context = LowerContext {
            signatures: &self.signatures,
            current_class: self.current_class.as_deref(),
            locals: &locals,
        };
        match lower::lower(callee, args.to_vec(), expr.span, &context) {
            Ok(rewritten) => rewritten,
            Err(error) => self.fail(error, expr),
        }
    }

    /// Records the class named by an assignment for later method-signature lookup.
    fn note_assignment(&self, name: &str, value: &Expr) {
        let mut locals = self.locals.borrow_mut();
        let class = match &value.kind {
            ExprKind::NewObject { class_name, .. } => Some(class_name.as_str().to_string()),
            ExprKind::Variable(other) => locals.get(other).cloned(),
            _ => None,
        };
        match class {
            Some(class) => {
                locals.insert(name.to_string(), class);
            }
            None => {
                locals.remove(name);
            }
        }
    }

    /// Saves the current local map and starts a fresh one.
    fn push_locals(&mut self) {
        let previous = self.locals.replace(HashMap::new());
        self.local_stack.borrow_mut().push(previous);
    }

    /// Restores the local map saved by `push_locals`.
    fn pop_locals(&mut self) {
        if let Some(previous) = self.local_stack.borrow_mut().pop() {
            self.locals.replace(previous);
        }
    }
}

/// Returns whether a named type hint names a class rather than a builtin pseudo-type.
fn copyable_class_hint(name: &str) -> bool {
    !matches!(
        name.trim_start_matches('\\').to_ascii_lowercase().as_str(),
        "self"
            | "parent"
            | "static"
            | "array"
            | "callable"
            | "iterable"
            | "object"
            | "mixed"
            | "string"
            | "int"
            | "float"
            | "bool"
            | "false"
            | "true"
            | "null"
            | "void"
            | "never"
    )
}

impl Pass for Desugar {
    /// Leaves magic constants unchanged.
    fn transform_magic(&self, _span: Span, magic: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(magic)
    }

    /// Rewrites calls after their arguments have been walked.
    fn transform_expr(&self, expr: Expr) -> Expr {
        match &expr.kind {
            ExprKind::FunctionCall { name, args } => {
                let callee = Callee::Function(name.clone());
                let args = args.clone();
                self.lower_call(expr, callee, &args)
            }
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => {
                let callee = Callee::StaticMethod {
                    receiver: receiver.clone(),
                    method: method.clone(),
                };
                let args = args.clone();
                self.lower_call(expr, callee, &args)
            }
            ExprKind::MethodCall {
                object,
                method,
                args,
            } => {
                let callee = Callee::Method {
                    object: (**object).clone(),
                    method: method.clone(),
                };
                let args = args.clone();
                self.lower_call(expr, callee, &args)
            }
            ExprKind::NullsafeMethodCall { args, .. } if args_contain_marker(args) => self.fail(
                CompileError::new(
                    expr.span,
                    "Cannot create Closure for a nullsafe partial application",
                ),
                expr,
            ),
            ExprKind::ExprCall { callee, args } => {
                let callee = Callee::Expr((**callee).clone());
                let args = args.clone();
                self.lower_call(expr, callee, &args)
            }
            ExprKind::ClosureCall { var, args } => {
                let callee = Callee::ClosureVar(var.clone());
                let args = args.clone();
                self.lower_call(expr, callee, &args)
            }
            ExprKind::NewObject { args, .. }
            | ExprKind::NewScopedObject { args, .. }
            | ExprKind::NewGeneric { args, .. }
            | ExprKind::NewDynamic { args, .. }
            | ExprKind::NewDynamicObject { args, .. }
                if args_contain_marker(args) =>
            {
                self.fail(
                    CompileError::new(expr.span, "Cannot create Closure for new expression"),
                    expr,
                )
            }
            ExprKind::NullsafeDynamicMethodCall { args, .. } if args_contain_marker(args) => self.fail(
                CompileError::new(
                    expr.span,
                    "Cannot create Closure for a dynamic method partial application",
                ),
                expr,
            ),
            _ => expr,
        }
    }

    /// Updates the local class map after each rebuilt statement.
    fn transform_stmt(&self, stmt: Stmt) -> Stmt {
        match &stmt.kind {
            StmtKind::Assign { name, value } => self.note_assignment(name, value),
            StmtKind::TypedAssign {
                type_expr,
                name,
                value,
            } => {
                let mut locals = self.locals.borrow_mut();
                if let crate::parser::ast::TypeExpr::Named(class_name) = type_expr {
                    if copyable_class_hint(class_name.as_str()) {
                        locals.insert(name.clone(), class_name.as_str().to_string());
                    } else {
                        locals.remove(name);
                    }
                } else {
                    let class = match &value.kind {
                        ExprKind::NewObject { class_name, .. } => {
                            Some(class_name.as_str().to_string())
                        }
                        _ => None,
                    };
                    match class {
                        Some(class) => {
                            locals.insert(name.clone(), class);
                        }
                        None => {
                            locals.remove(name);
                        }
                    }
                }
            }
            StmtKind::RefAssign { target, .. } => {
                self.locals.borrow_mut().remove(target);
            }
            StmtKind::Foreach {
                key_var,
                value_var,
                ..
            } => {
                let mut locals = self.locals.borrow_mut();
                if let Some(key) = key_var {
                    locals.remove(key);
                }
                locals.remove(value_var);
            }
            StmtKind::ListUnpack { vars, .. } => {
                let mut locals = self.locals.borrow_mut();
                for var in vars {
                    locals.remove(var);
                }
            }
            StmtKind::Global { vars } => {
                let mut locals = self.locals.borrow_mut();
                for var in vars {
                    locals.remove(var);
                }
            }
            StmtKind::StaticVar { name, .. } => {
                self.locals.borrow_mut().remove(name);
            }
            _ => {}
        }
        stmt
    }

    /// Enters a top-level function with no class receiver.
    fn enter_function(&mut self, _name: &str) {
        self.class_stack.push(self.current_class.take());
        self.push_locals();
    }

    /// Leaves a top-level function, restoring the enclosing class and locals.
    fn leave_function(&mut self) {
        self.pop_locals();
        self.current_class = self.class_stack.pop().flatten();
    }

    /// Enters a class-like declaration.
    fn enter_class(&mut self, name: &str) {
        self.class_stack.push(self.current_class.replace(name.to_string()));
    }

    /// Leaves a class-like declaration, restoring the previous receiver.
    fn leave_class(&mut self) {
        self.current_class = self.class_stack.pop().flatten();
    }

    /// Enters a trait declaration.
    fn enter_trait(&mut self, name: &str) {
        self.enter_class(name);
    }

    /// Leaves a trait declaration.
    fn leave_trait(&mut self) {
        self.leave_class();
    }

    /// Enters a method with a fresh local map but the enclosing class receiver.
    fn enter_method(&mut self, _name: &str, _type_params: &[crate::parser::ast::TypeParam]) {
        self.push_locals();
    }

    /// Leaves a method, restoring the enclosing locals.
    fn leave_method(&mut self) {
        self.pop_locals();
    }

    /// Enters a closure without losing the enclosing class.
    fn enter_closure(&mut self, _span: Span) {
        self.push_locals();
    }

    /// Leaves a closure, restoring the enclosing locals.
    fn leave_closure(&mut self) {
        self.pop_locals();
    }
}
