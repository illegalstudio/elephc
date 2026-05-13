//! Purpose:
//! Walks the AST to answer whether a named variable is referenced by codegen-relevant constructs.
//! Supports storage and optimization decisions that depend on variable visibility.
//!
//! Called from:
//! - `crate::codegen::program_usage`
//!
//! Key details:
//! - The traversal must include nested expressions, control-flow bodies, and instanceof targets.

use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget, Program, Stmt, StmtKind};

pub(in crate::codegen) fn program_uses_variable(program: &Program, needle: &str) -> bool {
    program.iter().any(|stmt| stmt_uses_variable(stmt, needle))
}

fn stmt_uses_variable(stmt: &Stmt, needle: &str) -> bool {
    match &stmt.kind {
        StmtKind::Synthetic(stmts) => stmts.iter().any(|stmt| stmt_uses_variable(stmt, needle)),
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::Echo(value)
        | StmtKind::Throw(value)
        | StmtKind::ExprStmt(value)
        | StmtKind::ConstDecl { value, .. } => expr_uses_variable(value, needle),
        StmtKind::Return(Some(value)) => expr_uses_variable(value, needle),
        StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => false,
        StmtKind::ArrayAssign { array, index, value } => {
            array == needle
                || expr_uses_variable(index, needle)
                || expr_uses_variable(value, needle)
        }
        StmtKind::ArrayPush { array, value } => {
            array == needle || expr_uses_variable(value, needle)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_uses_variable(condition, needle)
                || then_body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || elseif_clauses.iter().any(|(cond, body)| {
                    expr_uses_variable(cond, needle)
                        || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                })
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            then_body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
        }
        StmtKind::While { condition, body } => {
            expr_uses_variable(condition, needle)
                || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        StmtKind::DoWhile { body, condition } => {
            body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || expr_uses_variable(condition, needle)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_ref()
                .is_some_and(|stmt| stmt_uses_variable(stmt, needle))
                || condition
                    .as_ref()
                    .is_some_and(|expr| expr_uses_variable(expr, needle))
                || update
                    .as_ref()
                    .is_some_and(|stmt| stmt_uses_variable(stmt, needle))
                || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_uses_variable(array, needle)
                || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_uses_variable(subject, needle)
                || cases.iter().any(|(values, body)| {
                    values.iter().any(|expr| expr_uses_variable(expr, needle))
                        || body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || catches.iter().any(|catch_clause| {
                    catch_clause
                        .body
                        .iter()
                        .any(|stmt| stmt_uses_variable(stmt, needle))
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(|stmt| stmt_uses_variable(stmt, needle)))
        }
        StmtKind::ListUnpack { value, .. } => expr_uses_variable(value, needle),
        StmtKind::StaticVar { init, .. } => expr_uses_variable(init, needle),
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_uses_variable(object, needle) || expr_uses_variable(value, needle)
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_uses_variable(value, needle),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_uses_variable(index, needle) || expr_uses_variable(value, needle)
        }
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_uses_variable(object, needle) || expr_uses_variable(value, needle)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            expr_uses_variable(object, needle)
                || expr_uses_variable(index, needle)
                || expr_uses_variable(value, needle)
        }
        StmtKind::FunctionDecl { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => {
            body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. } => methods.iter().any(|method| {
            method
                .body
                .iter()
                .any(|stmt| stmt_uses_variable(stmt, needle))
        }),
        StmtKind::EnumDecl { cases, .. } => cases.iter().any(|case| {
            case.value
                .as_ref()
                .is_some_and(|expr| expr_uses_variable(expr, needle))
        }),
        StmtKind::Global { vars } => vars.iter().any(|name| name == needle),
        StmtKind::PackedClassDecl { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::Include { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,
    }
}

fn expr_uses_variable(expr: &Expr, needle: &str) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => name == needle,
        ExprKind::BinaryOp { left, right, .. } => {
            expr_uses_variable(left, needle) || expr_uses_variable(right, needle)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_uses_variable(value, needle) || instanceof_target_uses_variable(target, needle)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. } => expr_uses_variable(inner, needle),
        ExprKind::NullCoalesce { value, default } => {
            expr_uses_variable(value, needle) || expr_uses_variable(default, needle)
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            prelude.iter().any(|stmt| stmt_uses_variable(stmt, needle))
                || expr_uses_variable(target, needle)
                || expr_uses_variable(value, needle)
                || result_target
                    .as_deref()
                    .is_some_and(|target| expr_uses_variable(target, needle))
        }
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => name == needle,
        ExprKind::FunctionCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            args.iter().any(|arg| expr_uses_variable(arg, needle))
        }
        ExprKind::ExprCall { callee, args } => {
            expr_uses_variable(callee, needle)
                || args.iter().any(|arg| expr_uses_variable(arg, needle))
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(|item| expr_uses_variable(item, needle)),
        ExprKind::ArrayLiteralAssoc(items) => items.iter().any(|(key, value)| {
            expr_uses_variable(key, needle) || expr_uses_variable(value, needle)
        }),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_uses_variable(subject, needle)
                || arms.iter().any(|(values, value)| {
                    values.iter().any(|expr| expr_uses_variable(expr, needle))
                        || expr_uses_variable(value, needle)
                })
                || default
                    .as_ref()
                    .is_some_and(|expr| expr_uses_variable(expr, needle))
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_uses_variable(array, needle) || expr_uses_variable(index, needle)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_uses_variable(condition, needle)
                || expr_uses_variable(then_expr, needle)
                || expr_uses_variable(else_expr, needle)
        }
        ExprKind::ShortTernary { value, default } => {
            expr_uses_variable(value, needle) || expr_uses_variable(default, needle)
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::NamedArg { value: expr, .. }
        | ExprKind::BufferNew { len: expr, .. } => expr_uses_variable(expr, needle),
        ExprKind::Closure { body, .. } => {
            body.iter().any(|stmt| stmt_uses_variable(stmt, needle))
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_uses_variable(object, needle),
        ExprKind::StaticPropertyAccess { .. } => false,
        ExprKind::MethodCall { object, args, .. } => {
            expr_uses_variable(object, needle)
                || args.iter().any(|arg| expr_uses_variable(arg, needle))
        }
        ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_uses_variable(object, needle)
                || args.iter().any(|arg| expr_uses_variable(arg, needle))
        }
        ExprKind::FirstClassCallable(callable) => match callable {
            crate::parser::ast::CallableTarget::Function(_)
            | crate::parser::ast::CallableTarget::StaticMethod { .. } => false,
            crate::parser::ast::CallableTarget::Method { object, .. } => {
                expr_uses_variable(object, needle)
            }
        },
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::This => false,
        ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. } => false,
        ExprKind::NewScopedObject { args, .. } => {
            args.iter().any(|arg| expr_uses_variable(arg, needle))
        }
        ExprKind::Yield { key, value } => {
            key.as_ref().is_some_and(|k| expr_uses_variable(k, needle))
                || value.as_ref().is_some_and(|v| expr_uses_variable(v, needle))
        }
        ExprKind::YieldFrom(inner) => expr_uses_variable(inner, needle),
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before codegen analysis")
        }
    }
}

fn instanceof_target_uses_variable(target: &InstanceOfTarget, needle: &str) -> bool {
    match target {
        InstanceOfTarget::Name(_) => false,
        InstanceOfTarget::Expr(expr) => expr_uses_variable(expr, needle),
    }
}
