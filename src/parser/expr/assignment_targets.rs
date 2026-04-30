use std::collections::HashSet;

use crate::parser::ast::{Expr, ExprKind};

pub(super) fn is_non_local_assignment_target(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::ArrayAccess { .. }
            | ExprKind::PropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
    )
}

pub(super) fn is_assignment_expression_target(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_)
        | ExprKind::PropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, .. } => matches!(
            &array.kind,
            ExprKind::Variable(_)
                | ExprKind::PropertyAccess { .. }
                | ExprKind::StaticPropertyAccess { .. }
        ),
        _ => false,
    }
}

pub(super) fn assignment_value_may_mutate_target_dependency(
    target: &Expr,
    value: &Expr,
) -> bool {
    let mut dependencies = HashSet::new();
    collect_assignment_target_dependencies(target, &mut dependencies);
    !dependencies.is_empty() && expr_may_write_dependency(value, &dependencies)
}

fn collect_assignment_target_dependencies(expr: &Expr, dependencies: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Variable(name) => {
            dependencies.insert(name.clone());
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_assignment_target_dependencies(array, dependencies);
            collect_assignment_target_dependencies(index, dependencies);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. }
        | ExprKind::MethodCall { object, .. }
        | ExprKind::NullsafeMethodCall { object, .. } => {
            collect_assignment_target_dependencies(object, dependencies);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_assignment_target_dependencies(left, dependencies);
            collect_assignment_target_dependencies(right, dependencies);
        }
        ExprKind::InstanceOf { value, .. }
        | ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value) => collect_assignment_target_dependencies(value, dependencies),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            collect_assignment_target_dependencies(value, dependencies);
            collect_assignment_target_dependencies(default, dependencies);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_assignment_target_dependencies(condition, dependencies);
            collect_assignment_target_dependencies(then_expr, dependencies);
            collect_assignment_target_dependencies(else_expr, dependencies);
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::ExprCall { args, .. } => {
            for arg in args {
                collect_assignment_target_dependencies(arg, dependencies);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_assignment_target_dependencies(item, dependencies);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_assignment_target_dependencies(key, dependencies);
                collect_assignment_target_dependencies(value, dependencies);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_assignment_target_dependencies(subject, dependencies);
            for (patterns, arm_value) in arms {
                for pattern in patterns {
                    collect_assignment_target_dependencies(pattern, dependencies);
                }
                collect_assignment_target_dependencies(arm_value, dependencies);
            }
            if let Some(default) = default {
                collect_assignment_target_dependencies(default, dependencies);
            }
        }
        ExprKind::Closure { .. }
        | ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::Assignment { .. }
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::BufferNew { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::NewScopedObject { .. }
        | ExprKind::MagicConstant(_) => {}
    }
}

fn expr_may_write_dependency(expr: &Expr, dependencies: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::Assignment { target, value } => {
            assignment_target_may_write_dependency(target, dependencies)
                || expr_may_write_dependency(value, dependencies)
        }
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => dependencies.contains(name),
        ExprKind::FunctionCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::ExprCall { args, .. } => args.iter().any(|arg| {
            expr_contains_dependency(arg, dependencies)
                || expr_may_write_dependency(arg, dependencies)
        }),
        ExprKind::BinaryOp { left, right, .. } => {
            expr_may_write_dependency(left, dependencies)
                || expr_may_write_dependency(right, dependencies)
        }
        ExprKind::InstanceOf { value, .. }
        | ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value) => expr_may_write_dependency(value, dependencies),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_may_write_dependency(value, dependencies)
                || expr_may_write_dependency(default, dependencies)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_may_write_dependency(condition, dependencies)
                || expr_may_write_dependency(then_expr, dependencies)
                || expr_may_write_dependency(else_expr, dependencies)
        }
        ExprKind::ArrayLiteral(items) => {
            items.iter().any(|item| expr_may_write_dependency(item, dependencies))
        }
        ExprKind::ArrayLiteralAssoc(items) => items.iter().any(|(key, value)| {
            expr_may_write_dependency(key, dependencies)
                || expr_may_write_dependency(value, dependencies)
        }),
        ExprKind::ArrayAccess { array, index } => {
            expr_may_write_dependency(array, dependencies)
                || expr_may_write_dependency(index, dependencies)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_may_write_dependency(subject, dependencies)
                || arms.iter().any(|(patterns, arm_value)| {
                    patterns
                        .iter()
                        .any(|pattern| expr_may_write_dependency(pattern, dependencies))
                        || expr_may_write_dependency(arm_value, dependencies)
                })
                || default
                    .as_deref()
                    .is_some_and(|default| expr_may_write_dependency(default, dependencies))
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            expr_may_write_dependency(object, dependencies)
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_may_write_dependency(object, dependencies)
                || args.iter().any(|arg| {
                    expr_contains_dependency(arg, dependencies)
                        || expr_may_write_dependency(arg, dependencies)
                })
        }
        ExprKind::NewObject { args, .. } | ExprKind::NewScopedObject { args, .. } => {
            args.iter().any(|arg| {
                expr_contains_dependency(arg, dependencies)
                    || expr_may_write_dependency(arg, dependencies)
            })
        }
        ExprKind::BufferNew { len, .. } => expr_may_write_dependency(len, dependencies),
        ExprKind::Closure { .. }
        | ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::MagicConstant(_) => false,
    }
}

fn assignment_target_may_write_dependency(target: &Expr, dependencies: &HashSet<String>) -> bool {
    match &target.kind {
        ExprKind::Variable(name) => dependencies.contains(name),
        ExprKind::ArrayAccess { array, .. } => expr_contains_dependency(array, dependencies),
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            expr_contains_dependency(object, dependencies)
        }
        ExprKind::StaticPropertyAccess { .. } => false,
        _ => expr_contains_dependency(target, dependencies),
    }
}

fn expr_contains_dependency(expr: &Expr, dependencies: &HashSet<String>) -> bool {
    let mut found = HashSet::new();
    collect_assignment_target_dependencies(expr, &mut found);
    found.iter().any(|name| dependencies.contains(name))
}
