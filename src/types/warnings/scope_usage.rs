//! Purpose:
//! Emits checker warnings for scope usage cases.
//! Scans typed AST and checker metadata for suspicious but non-fatal program patterns.
//!
//! Called from:
//! - `crate::types::warnings`
//!
//! Key details:
//! - Warning analysis should preserve source spans and avoid rejecting programs that type checking accepted.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileWarning;
use crate::parser::ast::{ClassMethod, Expr, Stmt, StmtKind};
use crate::span::Span;

use super::expr_reads::{collect_closure_warnings_in_stmt, collect_expr_reads};

/// Tracks variable declarations and reads within a single function-like scope.
/// Used to detect unused variables by comparing declared names against read names.
#[derive(Default)]
pub(super) struct ScopeUsage {
    declared: HashMap<String, Span>,
    reads: HashSet<String>,
}

impl ScopeUsage {
    /// Records a variable declaration, keeping the first span encountered for each name.
    fn declare(&mut self, name: &str, span: Span) {
        self.declared.entry(name.to_string()).or_insert(span);
    }

    /// Records a variable read (access) within the current scope.
    pub(super) fn read(&mut self, name: &str) {
        self.reads.insert(name.to_string());
    }
}

/// Recursively scans top-level statements for functions, methods, and classes,
/// dispatching each to `analyze_function_like_scope` or `analyze_method_scope`.
/// Also handles namespace blocks and falls back to closure-level analysis for other statements.
pub(super) fn collect_function_like_warnings(stmts: &[Stmt], warnings: &mut Vec<CompileWarning>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                params,
                variadic,
                body,
                ..
            } => analyze_function_like_scope(params, variadic.as_ref(), body, stmt.span, warnings),
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::InterfaceDecl { methods, .. } => {
                for method in methods {
                    analyze_method_scope(method, warnings);
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_function_like_warnings(body, warnings),
            _ => collect_closure_warnings_in_stmt(stmt, warnings),
        }
    }
}

/// Collects variable reads within a single class method and emits unused variable warnings.
/// Skips methods without a body (e.g., abstract methods).
pub(super) fn analyze_method_scope(method: &ClassMethod, warnings: &mut Vec<CompileWarning>) {
    if !method.has_body {
        return;
    }
    analyze_function_like_scope(
        &method.params,
        method.variadic.as_ref(),
        &method.body,
        method.span,
        warnings,
    );
}

/// Analyzes a function's parameter list and body to detect unused variables.
/// Declares parameters as scoped variables, collects all reads from the body,
/// then emits an "Unused variable" warning for any declared variable that was never read.
/// Variables whose names start with underscore are ignored.
pub(super) fn analyze_function_like_scope(
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&String>,
    body: &[Stmt],
    declaration_span: Span,
    warnings: &mut Vec<CompileWarning>,
) {
    let mut scope = ScopeUsage::default();
    for (name, _, _, is_ref) in params {
        scope.declare(name, declaration_span);
        if *is_ref {
            scope.read(name);
        }
    }
    if let Some(name) = variadic {
        scope.declare(name, declaration_span);
    }
    collect_scope_reads(body, &mut scope, warnings);
    for (name, span) in scope.declared {
        if !scope.reads.contains(&name) && !name.starts_with('_') {
            warnings.push(CompileWarning::new(
                span,
                &format!("Unused variable: ${}", name),
            ));
        }
    }
}

/// Recursively walks statements and expressions within a function-like scope,
/// recording variable declarations and reads into the provided `ScopeUsage` tracker.
/// Emits warnings for suspicious patterns via `collect_expr_reads` and `collect_closure_warnings_in_stmt`.
pub(super) fn collect_scope_reads(
    stmts: &[Stmt],
    scope: &mut ScopeUsage,
    warnings: &mut Vec<CompileWarning>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Synthetic(stmts) => {
                collect_scope_reads(stmts, scope, warnings);
            }
            StmtKind::IncludeOnceMark { .. } => {}
            StmtKind::IncludeOnceGuard { body, .. } => {
                collect_scope_reads(body, scope, warnings);
            }
            StmtKind::Assign { name, value } => {
                collect_expr_reads(value, scope, warnings);
                scope.declare(name, stmt.span);
            }
            StmtKind::TypedAssign { name, value, .. } => {
                collect_expr_reads(value, scope, warnings);
                scope.declare(name, stmt.span);
            }
            StmtKind::ArrayAssign { array, index, value } => {
                scope.read(array);
                collect_expr_reads(index, scope, warnings);
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::NestedArrayAssign { target, value } => {
                collect_expr_reads(target, scope, warnings);
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::ArrayPush { array, value } => {
                scope.read(array);
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::Echo(expr)
            | StmtKind::Throw(expr)
            | StmtKind::ExprStmt(expr)
            | StmtKind::ConstDecl { value: expr, .. } => collect_expr_reads(expr, scope, warnings),
            StmtKind::Return(Some(expr)) => collect_expr_reads(expr, scope, warnings),
            StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => {}
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                collect_expr_reads(condition, scope, warnings);
                collect_scope_reads(then_body, scope, warnings);
                for (cond, body) in elseif_clauses {
                    collect_expr_reads(cond, scope, warnings);
                    collect_scope_reads(body, scope, warnings);
                }
                if let Some(body) = else_body {
                    collect_scope_reads(body, scope, warnings);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_scope_reads(then_body, scope, warnings);
                if let Some(body) = else_body {
                    collect_scope_reads(body, scope, warnings);
                }
            }
            StmtKind::While { condition, body } => {
                collect_expr_reads(condition, scope, warnings);
                collect_scope_reads(body, scope, warnings);
            }
            StmtKind::DoWhile { body, condition } => {
                collect_scope_reads(body, scope, warnings);
                collect_expr_reads(condition, scope, warnings);
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(stmt) = init {
                    collect_scope_reads(std::slice::from_ref(stmt), scope, warnings);
                }
                if let Some(expr) = condition {
                    collect_expr_reads(expr, scope, warnings);
                }
                if let Some(stmt) = update {
                    collect_scope_reads(std::slice::from_ref(stmt), scope, warnings);
                }
                collect_scope_reads(body, scope, warnings);
            }
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                body,
                ..
            } => {
                collect_expr_reads(array, scope, warnings);
                if let Some(name) = key_var {
                    scope.declare(name, stmt.span);
                }
                scope.declare(value_var, stmt.span);
                collect_scope_reads(body, scope, warnings);
            }
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                collect_expr_reads(subject, scope, warnings);
                for (values, body) in cases {
                    for value in values {
                        collect_expr_reads(value, scope, warnings);
                    }
                    collect_scope_reads(body, scope, warnings);
                }
                if let Some(body) = default {
                    collect_scope_reads(body, scope, warnings);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_scope_reads(try_body, scope, warnings);
                for catch_clause in catches {
                    if let Some(name) = &catch_clause.variable {
                        scope.declare(name, stmt.span);
                    }
                    collect_scope_reads(&catch_clause.body, scope, warnings);
                }
                if let Some(body) = finally_body {
                    collect_scope_reads(body, scope, warnings);
                }
            }
            StmtKind::ListUnpack { vars, value } => {
                collect_expr_reads(value, scope, warnings);
                for name in vars {
                    scope.declare(name, stmt.span);
                }
            }
            StmtKind::Global { vars } => {
                for name in vars {
                    scope.declare(name, stmt.span);
                }
            }
            StmtKind::StaticVar { name, init } => {
                collect_expr_reads(init, scope, warnings);
                scope.declare(name, stmt.span);
            }
            StmtKind::PropertyAssign {
                object,
                value,
                ..
            } => {
                collect_expr_reads(object, scope, warnings);
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::StaticPropertyAssign { value, .. } => {
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::StaticPropertyArrayPush { value, .. } => {
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
                collect_expr_reads(index, scope, warnings);
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::PropertyArrayPush {
                object,
                value,
                ..
            } => {
                collect_expr_reads(object, scope, warnings);
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::PropertyArrayAssign {
                object,
                index,
                value,
                ..
            } => {
                collect_expr_reads(object, scope, warnings);
                collect_expr_reads(index, scope, warnings);
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::FunctionDecl {
                params,
                variadic,
                body,
                ..
            } => analyze_function_like_scope(params, variadic.as_ref(), body, stmt.span, warnings),
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::InterfaceDecl { methods, .. } => {
                for method in methods {
                    analyze_method_scope(method, warnings);
                }
            }
            StmtKind::EnumDecl { .. }
            | StmtKind::PackedClassDecl { .. }
            | StmtKind::NamespaceDecl { .. }
            | StmtKind::NamespaceBlock { .. }
            | StmtKind::UseDecl { .. }
            | StmtKind::Include { .. }
            | StmtKind::FunctionVariantGroup { .. }
            | StmtKind::FunctionVariantMark { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. } => collect_closure_warnings_in_stmt(stmt, warnings),
        }
    }
}

/// Returns a list of free variable names (reads of variables not declared in the parameter list).
/// Used by closure capture analysis to determine which outer scope variables a closure references.
pub(super) fn collect_free_reads_in_function_like(
    body: &[Stmt],
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&String>,
) -> Vec<String> {
    let mut inner = ScopeUsage::default();
    for (name, _, _, _) in params {
        inner.declare(name, Span::dummy());
    }
    if let Some(name) = variadic {
        inner.declare(name, Span::dummy());
    }
    let mut nested_warnings = Vec::new();
    collect_scope_reads(body, &mut inner, &mut nested_warnings);
    inner
        .reads
        .into_iter()
        .filter(|name| !inner.declared.contains_key(name))
        .collect()
}
