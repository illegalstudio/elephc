//! Purpose:
//! Emits checker warnings for expr reads cases.
//! Scans typed AST and checker metadata for suspicious but non-fatal program patterns.
//!
//! Called from:
//! - `crate::types::warnings`
//!
//! Key details:
//! - Warning analysis should preserve source spans and avoid rejecting programs that type checking accepted.

use crate::errors::CompileWarning;
use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};

use super::scope_usage::{
    ScopeUsage, analyze_function_like_scope, analyze_method_scope, collect_free_reads_in_function_like,
};

/// Recursively collects variable read warnings by scanning an expression tree.
/// Records each variable reference into `scope` and emits warnings for suspicious patterns.
/// For closures, distinguishes between captured variables (captures list) and free reads
/// that must be resolved from the outer scope. Arrow closures collect free reads differently
/// than classic closures due to PHP semantics.
pub(super) fn collect_expr_reads(
    expr: &Expr,
    scope: &mut ScopeUsage,
    warnings: &mut Vec<CompileWarning>,
) {
    match &expr.kind {
        // `IncludeValue` is a transient parser node fully expanded by the resolver;
        // it can never reach this pass.
        ExprKind::IncludeValue { .. } => unreachable!(
            "ExprKind::IncludeValue must be expanded by the resolver"
        ),
        ExprKind::Variable(name) => scope.read(name),
        ExprKind::BinaryOp { left, right, .. } => {
            collect_expr_reads(left, scope, warnings);
            collect_expr_reads(right, scope, warnings);
        }
        ExprKind::InstanceOf { value, target } => {
            collect_expr_reads(value, scope, warnings);
            collect_instanceof_target_reads(target, scope, warnings);
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. } => collect_expr_reads(inner, scope, warnings),
        ExprKind::NullCoalesce { value, default } => {
            collect_expr_reads(value, scope, warnings);
            collect_expr_reads(default, scope, warnings);
        }
        ExprKind::Pipe { value, callable } => {
            collect_expr_reads(value, scope, warnings);
            collect_expr_reads(callable, scope, warnings);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            for stmt in prelude {
                collect_assignment_prelude_reads(stmt, scope, warnings);
            }
            if !matches!(target.kind, ExprKind::Variable(_)) {
                collect_expr_reads(target, scope, warnings);
            }
            collect_expr_reads(value, scope, warnings);
            if let Some(result_target) = result_target {
                collect_expr_reads(result_target, scope, warnings);
            }
        }
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => scope.read(name),
        ExprKind::ClosureCall { var, args } => {
            scope.read(var);
            for arg in args {
                collect_expr_reads(arg, scope, warnings);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ExprCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::MethodCall { args, .. }
        | ExprKind::NullsafeMethodCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            if let ExprKind::ExprCall { callee, .. } = &expr.kind {
                collect_expr_reads(callee, scope, warnings);
            }
            if let ExprKind::MethodCall { object, .. } = &expr.kind {
                collect_expr_reads(object, scope, warnings);
            }
            if let ExprKind::NullsafeMethodCall { object, .. } = &expr.kind {
                collect_expr_reads(object, scope, warnings);
            }
            for arg in args {
                collect_expr_reads(arg, scope, warnings);
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            collect_expr_reads(name_expr, scope, warnings);
            for arg in args {
                collect_expr_reads(arg, scope, warnings);
            }
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            collect_expr_reads(class_name, scope, warnings);
            for arg in args {
                collect_expr_reads(arg, scope, warnings);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_expr_reads(item, scope, warnings);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_expr_reads(key, scope, warnings);
                collect_expr_reads(value, scope, warnings);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_expr_reads(subject, scope, warnings);
            for (values, body) in arms {
                for value in values {
                    collect_expr_reads(value, scope, warnings);
                }
                collect_expr_reads(body, scope, warnings);
            }
            if let Some(default) = default {
                collect_expr_reads(default, scope, warnings);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_expr_reads(array, scope, warnings);
            collect_expr_reads(index, scope, warnings);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_reads(condition, scope, warnings);
            collect_expr_reads(then_expr, scope, warnings);
            collect_expr_reads(else_expr, scope, warnings);
        }
        ExprKind::ShortTernary { value, default } => {
            collect_expr_reads(value, scope, warnings);
            collect_expr_reads(default, scope, warnings);
        }
        ExprKind::Cast { expr, .. } => collect_expr_reads(expr, scope, warnings),
        ExprKind::Closure {
            params,
            variadic,
            body,
            captures,
            is_arrow,
            ..
        } => {
            if *is_arrow {
                for name in collect_free_reads_in_function_like(body, params, variadic.as_ref()) {
                    scope.read(&name);
                }
            }
            for name in captures {
                scope.read(name);
            }
            analyze_function_like_scope(params, variadic.as_ref(), body, expr.span, warnings);
        }
        ExprKind::NamedArg { value, .. } => collect_expr_reads(value, scope, warnings),
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_expr_reads(object, scope, warnings)
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_expr_reads(object, scope, warnings);
            collect_expr_reads(property, scope, warnings);
        }
        ExprKind::StaticPropertyAccess { .. } => {},
        ExprKind::BufferNew { len, .. } => collect_expr_reads(len, scope, warnings),
        ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. } => {}
        ExprKind::NewScopedObject { args, .. } => {
            for arg in args {
                collect_expr_reads(arg, scope, warnings);
            }
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This => {}
        ExprKind::Yield { key, value } => {
            if let Some(k) = key {
                collect_expr_reads(k, scope, warnings);
            }
            if let Some(v) = value {
                collect_expr_reads(v, scope, warnings);
            }
        }
        ExprKind::YieldFrom(inner) => collect_expr_reads(inner, scope, warnings),
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before warnings analysis")
        }
    }
}

/// Scans a synthetic or include-guarded statement for variable reads in assignment preludes.
/// Only `Assign` statements contribute reads; other statement kinds are skipped.
/// This handles the initializer expressions that run before a compound assignment completes.
fn collect_assignment_prelude_reads(
    stmt: &Stmt,
    scope: &mut ScopeUsage,
    warnings: &mut Vec<CompileWarning>,
) {
    match &stmt.kind {
        StmtKind::Synthetic(stmts) => {
            for stmt in stmts {
                collect_assignment_prelude_reads(stmt, scope, warnings);
            }
        }
        StmtKind::IncludeOnceGuard { body, .. } => {
            for stmt in body {
                collect_assignment_prelude_reads(stmt, scope, warnings);
            }
        }
        StmtKind::IncludeOnceMark { .. } => {}
        StmtKind::Assign { value, .. } => collect_expr_reads(value, scope, warnings),
        StmtKind::RefAssign { source, .. } => collect_expr_reads(source, scope, warnings),
        _ => {}
    }
}

/// Recursively collects variable reads from an instanceof target expression.
/// Only `InstanceOfTarget::Expr` contains a nested expression; other variants are no-ops.
fn collect_instanceof_target_reads(
    target: &InstanceOfTarget,
    scope: &mut ScopeUsage,
    warnings: &mut Vec<CompileWarning>,
) {
    if let InstanceOfTarget::Expr(expr) = target {
        collect_expr_reads(expr, scope, warnings);
    }
}

/// Entry point for statement-level warning analysis. Walks a statement tree (including synthetic
/// and include-guarded statements) and collects variable reads by instantiating a fresh `ScopeUsage`
/// per distinct scope region. This isolation ensures reads in one branch do not bleed into others,
/// which is required for accurate warning attribution in control-flow structures.
pub(super) fn collect_closure_warnings_in_stmt(stmt: &Stmt, warnings: &mut Vec<CompileWarning>) {
    match &stmt.kind {
        StmtKind::Synthetic(stmts) => {
            for stmt in stmts {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
        }
        StmtKind::IncludeOnceGuard { body, .. } => {
            for stmt in body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
        }
        StmtKind::IncludeOnceMark { .. } => {}
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. } => {
            collect_expr_reads(expr, &mut ScopeUsage::default(), warnings);
        }
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ArrayPush { value, .. }
        | StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. }
        | StmtKind::Return(Some(value)) => {
            collect_expr_reads(value, &mut ScopeUsage::default(), warnings);
        }
        StmtKind::RefAssign { source, .. } => {
            let mut scope = ScopeUsage::default();
            collect_expr_reads(source, &mut scope, warnings);
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            let mut scope = ScopeUsage::default();
            collect_expr_reads(index, &mut scope, warnings);
            collect_expr_reads(value, &mut scope, warnings);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            let mut scope = ScopeUsage::default();
            collect_expr_reads(target, &mut scope, warnings);
            collect_expr_reads(value, &mut scope, warnings);
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            let mut scope = ScopeUsage::default();
            collect_expr_reads(object, &mut scope, warnings);
            collect_expr_reads(value, &mut scope, warnings);
        }
        StmtKind::PropertyArrayPush { object, value, .. } => {
            let mut scope = ScopeUsage::default();
            collect_expr_reads(object, &mut scope, warnings);
            collect_expr_reads(value, &mut scope, warnings);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            let mut scope = ScopeUsage::default();
            collect_expr_reads(object, &mut scope, warnings);
            collect_expr_reads(index, &mut scope, warnings);
            collect_expr_reads(value, &mut scope, warnings);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let mut scope = ScopeUsage::default();
            collect_expr_reads(condition, &mut scope, warnings);
            for stmt in then_body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
            for (cond, body) in elseif_clauses {
                collect_expr_reads(cond, &mut scope, warnings);
                for stmt in body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
        }
        StmtKind::DoWhile { body, condition } | StmtKind::While { body, condition } => {
            for stmt in body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
            collect_expr_reads(condition, &mut ScopeUsage::default(), warnings);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(stmt) = init {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
            if let Some(expr) = condition {
                collect_expr_reads(expr, &mut ScopeUsage::default(), warnings);
            }
            if let Some(stmt) = update {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
            for stmt in body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_expr_reads(array, &mut ScopeUsage::default(), warnings);
            for stmt in body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_expr_reads(subject, &mut ScopeUsage::default(), warnings);
            for (values, body) in cases {
                let mut scope = ScopeUsage::default();
                for value in values {
                    collect_expr_reads(value, &mut scope, warnings);
                }
                for stmt in body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
            if let Some(body) = default {
                for stmt in body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            for stmt in try_body {
                collect_closure_warnings_in_stmt(stmt, warnings);
            }
            for catch_clause in catches {
                for stmt in &catch_clause.body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
            if let Some(body) = finally_body {
                for stmt in body {
                    collect_closure_warnings_in_stmt(stmt, warnings);
                }
            }
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
        | StmtKind::UseDecl { .. }
        | StmtKind::Include { .. }
        | StmtKind::Global { .. }
        | StmtKind::StaticVar { .. }
        | StmtKind::Return(None)
        | StmtKind::ListUnpack { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. } => {}
        StmtKind::NamespaceBlock { body, .. } => super::scope_usage::collect_function_like_warnings(body, warnings),
    }
}
