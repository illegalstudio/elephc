use crate::errors::CompileWarning;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};

use super::scope_usage::{
    ScopeUsage, analyze_function_like_scope, analyze_method_scope, collect_free_reads_in_function_like,
};

pub(super) fn collect_expr_reads(
    expr: &Expr,
    scope: &mut ScopeUsage,
    warnings: &mut Vec<CompileWarning>,
) {
    match &expr.kind {
        ExprKind::Variable(name) => scope.read(name),
        ExprKind::BinaryOp { left, right, .. } => {
            collect_expr_reads(left, scope, warnings);
            collect_expr_reads(right, scope, warnings);
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. } => collect_expr_reads(inner, scope, warnings),
        ExprKind::NullCoalesce { value, default } => {
            collect_expr_reads(value, scope, warnings);
            collect_expr_reads(default, scope, warnings);
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
        | ExprKind::StaticMethodCall { args, .. } => {
            if let ExprKind::ExprCall { callee, .. } = &expr.kind {
                collect_expr_reads(callee, scope, warnings);
            }
            if let ExprKind::MethodCall { object, .. } = &expr.kind {
                collect_expr_reads(object, scope, warnings);
            }
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
        ExprKind::PropertyAccess { object, .. } => collect_expr_reads(object, scope, warnings),
        ExprKind::BufferNew { len, .. } => collect_expr_reads(len, scope, warnings),
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This => {}
    }
}

pub(super) fn collect_closure_warnings_in_stmt(stmt: &Stmt, warnings: &mut Vec<CompileWarning>) {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. } => {
            collect_expr_reads(expr, &mut ScopeUsage::default(), warnings);
        }
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ArrayPush { value, .. }
        | StmtKind::Return(Some(value)) => {
            collect_expr_reads(value, &mut ScopeUsage::default(), warnings);
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            let mut scope = ScopeUsage::default();
            collect_expr_reads(index, &mut scope, warnings);
            collect_expr_reads(value, &mut scope, warnings);
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            let mut scope = ScopeUsage::default();
            collect_expr_reads(object, &mut scope, warnings);
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
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
        StmtKind::NamespaceBlock { body, .. } => super::scope_usage::collect_function_like_warnings(body, warnings),
    }
}
