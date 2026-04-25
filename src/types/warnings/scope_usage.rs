use std::collections::{HashMap, HashSet};

use crate::errors::CompileWarning;
use crate::parser::ast::{ClassMethod, Expr, Stmt, StmtKind};
use crate::span::Span;

use super::expr_reads::{collect_closure_warnings_in_stmt, collect_expr_reads};

#[derive(Default)]
pub(super) struct ScopeUsage {
    declared: HashMap<String, Span>,
    reads: HashSet<String>,
}

impl ScopeUsage {
    fn declare(&mut self, name: &str, span: Span) {
        self.declared.entry(name.to_string()).or_insert(span);
    }

    pub(super) fn read(&mut self, name: &str) {
        self.reads.insert(name.to_string());
    }
}

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

pub(super) fn collect_scope_reads(
    stmts: &[Stmt],
    scope: &mut ScopeUsage,
    warnings: &mut Vec<CompileWarning>,
) {
    for stmt in stmts {
        match &stmt.kind {
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
            StmtKind::ArrayPush { array, value } => {
                scope.read(array);
                collect_expr_reads(value, scope, warnings);
            }
            StmtKind::Echo(expr)
            | StmtKind::Throw(expr)
            | StmtKind::ExprStmt(expr)
            | StmtKind::ConstDecl { value: expr, .. } => collect_expr_reads(expr, scope, warnings),
            StmtKind::Return(Some(expr)) => collect_expr_reads(expr, scope, warnings),
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
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
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. } => collect_closure_warnings_in_stmt(stmt, warnings),
        }
    }
}

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
