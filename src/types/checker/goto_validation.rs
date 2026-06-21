//! Purpose:
//! Validates `goto`/`label:` usage per function-level label scope: every `goto` must target a
//! label defined in the same scope, and no label name may be defined twice in one scope.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl` (alongside the other whole-program checks).
//!
//! Key details:
//! - PHP labels are scoped to the enclosing function/method/closure body; the global program forms
//!   its own scope. Each scope is validated independently, so the same label name may appear in two
//!   different functions without conflict.
//! - `goto`/`label` are statements and never appear inside expressions, so label collection only
//!   walks statement sub-bodies; expressions are scanned solely to discover nested closure scopes.
//! - Surfacing these as `CompileError`s here keeps an undefined `goto` target or a duplicate label
//!   from reaching EIR lowering, where it would otherwise fail an internal block-terminator check.

use std::collections::HashSet;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use crate::span::Span;

/// Validates every label scope in `program` and returns all collected diagnostics.
///
/// Begins with the global scope (the top-level statement list) and recurses into each nested
/// function, method, and closure body as an independent scope.
pub(crate) fn validate_goto_labels(program: &Program) -> Vec<CompileError> {
    let mut errors = Vec::new();
    validate_scope(program, &mut errors);
    errors
}

/// Validates one label scope: collects the labels and `goto`s defined directly in `body` (without
/// crossing into nested function/closure scopes), reports duplicate labels and `goto`s to undefined
/// labels, then recurses into the nested scopes discovered while walking.
fn validate_scope(body: &[Stmt], errors: &mut Vec<CompileError>) {
    let mut labels: Vec<(String, Span)> = Vec::new();
    let mut gotos: Vec<(String, Span)> = Vec::new();
    for stmt in body {
        collect_and_recurse_stmt(stmt, &mut labels, &mut gotos, errors);
    }

    let mut defined: HashSet<&str> = HashSet::new();
    for (name, span) in &labels {
        if !defined.insert(name.as_str()) {
            errors.push(CompileError::new(
                *span,
                &format!("label '{name}' already defined"),
            ));
        }
    }
    for (name, span) in &gotos {
        if !defined.contains(name.as_str()) {
            errors.push(CompileError::new(
                *span,
                &format!("'goto' to undefined label '{name}'"),
            ));
        }
    }
}

/// Collects `goto`/`label` statements belonging to the current scope from `stmt`, recursing through
/// structured control-flow sub-bodies, and validates any nested function/method/closure scope it
/// encounters as a fresh scope. Expressions are scanned only to discover nested closures.
fn collect_and_recurse_stmt(
    stmt: &Stmt,
    labels: &mut Vec<(String, Span)>,
    gotos: &mut Vec<(String, Span)>,
    errors: &mut Vec<CompileError>,
) {
    match &stmt.kind {
        StmtKind::Label(name) => labels.push((name.clone(), stmt.span)),
        StmtKind::Goto(name) => gotos.push((name.clone(), stmt.span)),
        // Nested scopes: validated independently; their labels do not belong to this scope.
        StmtKind::FunctionDecl { body, .. } => validate_scope(body, errors),
        StmtKind::ClassDecl { methods, .. } | StmtKind::TraitDecl { methods, .. } => {
            for method in methods {
                if method.has_body {
                    validate_scope(&method.body, errors);
                }
            }
        }
        StmtKind::InterfaceDecl { .. } => {}
        // Structured control flow: recurse, staying in the current scope.
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            scan_expr_for_closures(condition, errors);
            collect_in_block(then_body, labels, gotos, errors);
            for (cond, body) in elseif_clauses {
                scan_expr_for_closures(cond, errors);
                collect_in_block(body, labels, gotos, errors);
            }
            if let Some(else_body) = else_body {
                collect_in_block(else_body, labels, gotos, errors);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_in_block(then_body, labels, gotos, errors);
            if let Some(else_body) = else_body {
                collect_in_block(else_body, labels, gotos, errors);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            scan_expr_for_closures(condition, errors);
            collect_in_block(body, labels, gotos, errors);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_and_recurse_stmt(init, labels, gotos, errors);
            }
            if let Some(condition) = condition {
                scan_expr_for_closures(condition, errors);
            }
            if let Some(update) = update {
                collect_and_recurse_stmt(update, labels, gotos, errors);
            }
            collect_in_block(body, labels, gotos, errors);
        }
        StmtKind::Foreach { array, body, .. } => {
            scan_expr_for_closures(array, errors);
            collect_in_block(body, labels, gotos, errors);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            scan_expr_for_closures(subject, errors);
            for (values, body) in cases {
                for value in values {
                    scan_expr_for_closures(value, errors);
                }
                collect_in_block(body, labels, gotos, errors);
            }
            if let Some(default) = default {
                collect_in_block(default, labels, gotos, errors);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_in_block(try_body, labels, gotos, errors);
            for catch in catches {
                collect_in_block(&catch.body, labels, gotos, errors);
            }
            if let Some(finally_body) = finally_body {
                collect_in_block(finally_body, labels, gotos, errors);
            }
        }
        StmtKind::Synthetic(stmts)
        | StmtKind::NamespaceBlock { body: stmts, .. }
        | StmtKind::IncludeOnceGuard { body: stmts, .. } => {
            collect_in_block(stmts, labels, gotos, errors);
        }
        // Expression-bearing statements: no labels/gotos, but may host nested closures.
        StmtKind::Echo(expr) | StmtKind::ExprStmt(expr) | StmtKind::Throw(expr) => {
            scan_expr_for_closures(expr, errors);
        }
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ConstDecl { value, .. }
        | StmtKind::ListUnpack { value, .. }
        | StmtKind::ArrayPush { value, .. }
        | StmtKind::StaticVar { init: value, .. } => scan_expr_for_closures(value, errors),
        StmtKind::ArrayAssign { index, value, .. } => {
            scan_expr_for_closures(index, errors);
            scan_expr_for_closures(value, errors);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            scan_expr_for_closures(target, errors);
            scan_expr_for_closures(value, errors);
        }
        StmtKind::RefAssignTarget { target, .. } => scan_expr_for_closures(target, errors),
        StmtKind::Return(Some(expr)) => scan_expr_for_closures(expr, errors),
        StmtKind::Include { path, .. } => scan_expr_for_closures(path, errors),
        // Leaves with no labels, sub-bodies, or closure-bearing expressions.
        _ => {}
    }
}

/// Collects labels and `goto`s from a nested statement block, staying in the current scope.
fn collect_in_block(
    body: &[Stmt],
    labels: &mut Vec<(String, Span)>,
    gotos: &mut Vec<(String, Span)>,
    errors: &mut Vec<CompileError>,
) {
    for stmt in body {
        collect_and_recurse_stmt(stmt, labels, gotos, errors);
    }
}

/// Walks an expression looking only for `Closure` bodies, validating each as its own label scope.
///
/// Labels and `goto`s never appear inside expressions, so nothing is collected here; the walk
/// exists solely to discover closure scopes nested within expression positions.
fn scan_expr_for_closures(expr: &Expr, errors: &mut Vec<CompileError>) {
    match &expr.kind {
        ExprKind::Closure { body, .. } => validate_scope(body, errors),
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::NullCoalesce {
            value: left,
            default: right,
        }
        | ExprKind::ShortTernary {
            value: left,
            default: right,
        }
        | ExprKind::Pipe {
            value: left,
            callable: right,
        }
        | ExprKind::ArrayAccess {
            array: left,
            index: right,
        }
        | ExprKind::Assignment {
            target: left,
            value: right,
            ..
        } => {
            scan_expr_for_closures(left, errors);
            scan_expr_for_closures(right, errors);
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Clone(inner)
        | ExprKind::Print(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::InstanceOf { value: inner, .. } => scan_expr_for_closures(inner, errors),
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            for arg in args {
                scan_expr_for_closures(arg, errors);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            scan_expr_for_closures(callee, errors);
            for arg in args {
                scan_expr_for_closures(arg, errors);
            }
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            scan_expr_for_closures(object, errors);
            for arg in args {
                scan_expr_for_closures(arg, errors);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                scan_expr_for_closures(item, errors);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                scan_expr_for_closures(key, errors);
                scan_expr_for_closures(value, errors);
            }
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            scan_expr_for_closures(condition, errors);
            scan_expr_for_closures(then_expr, errors);
            scan_expr_for_closures(else_expr, errors);
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            scan_expr_for_closures(subject, errors);
            for (patterns, value) in arms {
                for pattern in patterns {
                    scan_expr_for_closures(pattern, errors);
                }
                scan_expr_for_closures(value, errors);
            }
            if let Some(default) = default {
                scan_expr_for_closures(default, errors);
            }
        }
        ExprKind::NamedArg { value, .. } => scan_expr_for_closures(value, errors),
        // Remaining expression kinds host no closures relevant to label scoping.
        _ => {}
    }
}
