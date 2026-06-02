//! Purpose:
//! Detects instanceof expressions whose target is computed at runtime.
//! Signals when codegen must keep broader class metadata available for dynamic checks.
//!
//! Called from:
//! - `crate::codegen::program_usage::required_classes`
//!
//! Key details:
//! - Dynamic targets require conservative metadata retention because the concrete class is not known statically.

use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget, Program, Stmt, StmtKind};

/// Returns true if the program contains any `instanceof` expression whose target is
/// computed at runtime (not a statically-known class name).
///
/// A dynamic target means the concrete class cannot be determined at compile time,
/// so codegen must retain broader class metadata for the check.
pub(in crate::codegen) fn program_has_dynamic_instanceof(program: &Program) -> bool {
    body_has_dynamic_instanceof(program)
}

/// Scans a statement list and returns true if any statement contains a dynamic instanceof.
fn body_has_dynamic_instanceof(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_has_dynamic_instanceof)
}

/// Scans a single statement for a dynamic instanceof, recursing into class/function/try bodies.
fn stmt_has_dynamic_instanceof(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. } => methods
            .iter()
            .any(|method| body_has_dynamic_instanceof(&method.body)),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_has_dynamic_instanceof(try_body)
                || catches
                    .iter()
                    .any(|catch_clause| body_has_dynamic_instanceof(&catch_clause.body))
                || finally_body
                    .as_deref()
                    .is_some_and(body_has_dynamic_instanceof)
        }
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => body_has_dynamic_instanceof(body),
        StmtKind::FunctionDecl { body, .. } => body_has_dynamic_instanceof(body),
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            body_has_dynamic_instanceof(then_body)
                || else_body
                    .as_deref()
                    .is_some_and(body_has_dynamic_instanceof)
        }
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::Return(Some(expr))
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::PropertyAssign { value: expr, .. }
        | StmtKind::PropertyArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. } => {
            expr_has_dynamic_instanceof(expr)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_has_dynamic_instanceof(condition)
                || body_has_dynamic_instanceof(then_body)
                || elseif_clauses.iter().any(|(condition, body)| {
                    expr_has_dynamic_instanceof(condition) || body_has_dynamic_instanceof(body)
                })
                || else_body
                    .as_deref()
                    .is_some_and(body_has_dynamic_instanceof)
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_has_dynamic_instanceof(condition) || body_has_dynamic_instanceof(body)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_has_dynamic_instanceof)
                || condition
                    .as_ref()
                    .is_some_and(expr_has_dynamic_instanceof)
                || update.as_deref().is_some_and(stmt_has_dynamic_instanceof)
                || body_has_dynamic_instanceof(body)
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_has_dynamic_instanceof(array) || body_has_dynamic_instanceof(body)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_has_dynamic_instanceof(subject)
                || cases.iter().any(|(patterns, body)| {
                    patterns.iter().any(expr_has_dynamic_instanceof)
                        || body_has_dynamic_instanceof(body)
                })
                || default.as_deref().is_some_and(body_has_dynamic_instanceof)
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_has_dynamic_instanceof(index) || expr_has_dynamic_instanceof(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_has_dynamic_instanceof(target) || expr_has_dynamic_instanceof(value)
        }
        _ => false,
    }
}

/// Scans an expression tree for an instanceof with a dynamic target or nested dynamic instanceof.
fn expr_has_dynamic_instanceof(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::InstanceOf { value, target } => {
            matches!(target, InstanceOfTarget::Expr(_))
                || expr_has_dynamic_instanceof(value)
                || match target {
                    InstanceOfTarget::Name(_) => false,
                    InstanceOfTarget::Expr(expr) => expr_has_dynamic_instanceof(expr),
                }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_has_dynamic_instanceof(left) || expr_has_dynamic_instanceof(right)
        }
        ExprKind::Negate(expr)
        | ExprKind::Not(expr)
        | ExprKind::BitNot(expr)
        | ExprKind::Throw(expr)
        | ExprKind::ErrorSuppress(expr)
        | ExprKind::Print(expr)
        | ExprKind::Spread(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::PtrCast { expr, .. }
        | ExprKind::NamedArg { value: expr, .. }
        | ExprKind::BufferNew { len: expr, .. } => expr_has_dynamic_instanceof(expr),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_has_dynamic_instanceof(value) || expr_has_dynamic_instanceof(default)
        }
        ExprKind::Pipe { value, callable } => {
            expr_has_dynamic_instanceof(value) || expr_has_dynamic_instanceof(callable)
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            body_has_dynamic_instanceof(prelude)
                || expr_has_dynamic_instanceof(target)
                || expr_has_dynamic_instanceof(value)
                || result_target
                    .as_deref()
                    .is_some_and(expr_has_dynamic_instanceof)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. } => args.iter().any(expr_has_dynamic_instanceof),
        ExprKind::NewDynamic { name_expr, args } => {
            expr_has_dynamic_instanceof(name_expr) || args.iter().any(expr_has_dynamic_instanceof)
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            expr_has_dynamic_instanceof(class_name)
                || args.iter().any(expr_has_dynamic_instanceof)
        }
        ExprKind::ExprCall { callee, args } => {
            expr_has_dynamic_instanceof(callee) || args.iter().any(expr_has_dynamic_instanceof)
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_has_dynamic_instanceof),
        ExprKind::ArrayLiteralAssoc(items) => items.iter().any(|(key, value)| {
            expr_has_dynamic_instanceof(key) || expr_has_dynamic_instanceof(value)
        }),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_has_dynamic_instanceof(subject)
                || arms.iter().any(|(patterns, result)| {
                    patterns.iter().any(expr_has_dynamic_instanceof)
                        || expr_has_dynamic_instanceof(result)
                })
                || default
                    .as_deref()
                    .is_some_and(expr_has_dynamic_instanceof)
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_has_dynamic_instanceof(array) || expr_has_dynamic_instanceof(index)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_has_dynamic_instanceof(condition)
                || expr_has_dynamic_instanceof(then_expr)
                || expr_has_dynamic_instanceof(else_expr)
        }
        ExprKind::Closure { body, .. } => body_has_dynamic_instanceof(body),
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_has_dynamic_instanceof(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_has_dynamic_instanceof(object) || expr_has_dynamic_instanceof(property)
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_has_dynamic_instanceof(object) || args.iter().any(expr_has_dynamic_instanceof)
        }
        ExprKind::FirstClassCallable(crate::parser::ast::CallableTarget::Method {
            object,
            ..
        }) => expr_has_dynamic_instanceof(object),
        ExprKind::NewScopedObject { args, .. } => args.iter().any(expr_has_dynamic_instanceof),
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. } => false,
        ExprKind::Yield { key, value } => {
            key.as_ref().is_some_and(|k| expr_has_dynamic_instanceof(k))
                || value
                    .as_ref()
                    .is_some_and(|v| expr_has_dynamic_instanceof(v))
        }
        ExprKind::YieldFrom(inner) => expr_has_dynamic_instanceof(inner),
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before codegen analysis")
        }
    }
}
