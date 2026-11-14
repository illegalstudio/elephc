//! Purpose:
//! Detects by-reference closure captures in one lowered function body before map-read fusion.
//!
//! Called from:
//! - `crate::ir_lower::stmt::map_read_fusion::fuse_map_reads`.
//!
//! Key details:
//! - A `use (&$values)` capture lets an earlier closure mutate a map source after `array_map()`
//!   evaluated it, so a deferred read would observe live data instead of the saved map.
//! - The walk covers every expression and statement shape that can hold nested expressions or
//!   blocks. Adding an AST variant is a compile error here, never a silent missed alias.
//! - Any captured reference disables fusion for the whole body, which is conservative but keeps
//!   the optimization fail-closed.

use crate::parser::ast::{CallableTarget, Expr, ExprKind, Stmt, StmtKind};

/// Returns whether `body` contains a closure that captures a local by reference.
pub(super) fn body_has_reference_capture(body: &[Stmt]) -> bool {
    body.iter().any(stmt_has_reference_capture)
}

/// Returns whether one statement holds a by-reference closure capture.
fn stmt_has_reference_capture(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::Include { path: expr, .. }
        | StmtKind::ExprStmt(expr) => expr_has_reference_capture(expr),
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::NestedArrayAssign { value, .. }
        | StmtKind::ArrayPush { value, .. }
        | StmtKind::ConstDecl { value, .. }
        | StmtKind::ListUnpack { value, .. }
        | StmtKind::StaticVar { init: value, .. } => expr_has_reference_capture(value),
        StmtKind::RefAssign { source, .. } => expr_has_reference_capture(source),
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_has_reference_capture(index) || expr_has_reference_capture(value)
        }
        StmtKind::Return(Some(expr)) => expr_has_reference_capture(expr),
        StmtKind::Return(None)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::Global { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_has_reference_capture(condition)
                || body_has_reference_capture(then_body)
                || elseif_clauses.iter().any(|(condition, body)| {
                    expr_has_reference_capture(condition) || body_has_reference_capture(body)
                })
                || else_body.as_deref().is_some_and(body_has_reference_capture)
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            body_has_reference_capture(then_body)
                || else_body.as_deref().is_some_and(body_has_reference_capture)
        }
        StmtKind::While { condition, body } => {
            expr_has_reference_capture(condition) || body_has_reference_capture(body)
        }
        StmtKind::DoWhile { body, condition } => {
            body_has_reference_capture(body) || expr_has_reference_capture(condition)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_has_reference_capture)
                || condition.as_ref().is_some_and(expr_has_reference_capture)
                || update.as_deref().is_some_and(stmt_has_reference_capture)
                || body_has_reference_capture(body)
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_has_reference_capture(array) || body_has_reference_capture(body)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_has_reference_capture(subject)
                || cases.iter().any(|(patterns, body)| {
                    patterns.iter().any(expr_has_reference_capture)
                        || body_has_reference_capture(body)
                })
                || default.as_deref().is_some_and(body_has_reference_capture)
        }
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body_has_reference_capture(body),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_has_reference_capture(try_body)
                || catches
                    .iter()
                    .any(|catch| body_has_reference_capture(&catch.body))
                || finally_body.as_deref().is_some_and(body_has_reference_capture)
        }
        StmtKind::FunctionDecl {
            params, body, ..
        } => {
            params.iter().any(|(_, _, default, _)| {
                default.as_ref().is_some_and(expr_has_reference_capture)
            }) || body_has_reference_capture(body)
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_has_reference_capture(object) || expr_has_reference_capture(value)
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_has_reference_capture(value),
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            expr_has_reference_capture(object)
                || expr_has_reference_capture(index)
                || expr_has_reference_capture(value)
        }
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_has_reference_capture(index) || expr_has_reference_capture(value)
        }
        // Class-like declarations can carry method bodies and property defaults. They are not
        // lowered inside another function body, so failing closed keeps the guard honest.
        StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. } => true,
    }
}

/// Returns whether one expression holds a by-reference closure capture.
fn expr_has_reference_capture(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::BinaryOp { left, right, .. } => {
            expr_has_reference_capture(left) || expr_has_reference_capture(right)
        }
        ExprKind::InstanceOf { value, .. } => expr_has_reference_capture(value),
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::IncludeValue { path: inner, .. }
        | ExprKind::Clone(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::YieldFrom(inner) => expr_has_reference_capture(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_has_reference_capture(value) || expr_has_reference_capture(default)
        }
        ExprKind::Pipe { value, callable } => {
            expr_has_reference_capture(value) || expr_has_reference_capture(callable)
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_has_reference_capture(target)
                || expr_has_reference_capture(value)
                || result_target
                    .as_deref()
                    .is_some_and(expr_has_reference_capture)
                || body_has_reference_capture(prelude)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ArrayLiteral(args)
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewGeneric { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::ClosureCall { args, .. } => args.iter().any(expr_has_reference_capture),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| {
                expr_has_reference_capture(key) || expr_has_reference_capture(value)
            }),
        ExprKind::ArrayLiteralMixed(items) => items
            .iter()
            .flat_map(|entry| entry.exprs())
            .any(expr_has_reference_capture),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_has_reference_capture(subject)
                || arms.iter().any(|(patterns, result)| {
                    patterns.iter().any(expr_has_reference_capture)
                        || expr_has_reference_capture(result)
                })
                || default
                    .as_deref()
                    .is_some_and(expr_has_reference_capture)
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_has_reference_capture(array) || expr_has_reference_capture(index)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_has_reference_capture(condition)
                || expr_has_reference_capture(then_expr)
                || expr_has_reference_capture(else_expr)
        }
        ExprKind::Closure {
            params,
            body,
            capture_refs,
            ..
        } => {
            !capture_refs.is_empty()
                || params.iter().any(|(_, _, default, _)| {
                    default.as_ref().is_some_and(expr_has_reference_capture)
                })
                || body_has_reference_capture(body)
        }
        ExprKind::ExprCall { callee, args } => {
            expr_has_reference_capture(callee)
                || args.iter().any(expr_has_reference_capture)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_has_reference_capture(name_expr)
                || args.iter().any(expr_has_reference_capture)
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            expr_has_reference_capture(class_name)
                || args.iter().any(expr_has_reference_capture)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            expr_has_reference_capture(object)
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_has_reference_capture(object) || expr_has_reference_capture(property)
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_has_reference_capture(object)
                || args.iter().any(expr_has_reference_capture)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            expr_has_reference_capture(object)
                || expr_has_reference_capture(method)
                || args.iter().any(expr_has_reference_capture)
        }
        ExprKind::StaticMethodCall { args, .. } => args.iter().any(expr_has_reference_capture),
        ExprKind::FirstClassCallable(target) => match target {
            CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => false,
            CallableTarget::Method { object, .. } => expr_has_reference_capture(object),
        },
        ExprKind::BufferNew { len, .. } => expr_has_reference_capture(len),
        ExprKind::ObjectClassName { object } => expr_has_reference_capture(object),
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_has_reference_capture)
                || value.as_deref().is_some_and(expr_has_reference_capture)
        }
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
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => false,
    }
}
