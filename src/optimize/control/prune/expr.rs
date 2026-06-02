//! Purpose:
//! Prunes constant control-flow expr cases.
//! Rewrites statements or expressions whose compile-time condition is known while preserving required effects.
//!
//! Called from:
//! - `crate::optimize::control::prune`
//!
//! Key details:
//! - Loop exits, empty bodies, and effectful conditions must be handled before removing structural statements.

use super::super::*;
use super::statements::{prune_block, prune_stmt};

/// Recursively rewrites an expression tree, pruning constant subexpressions while
/// preserving side effects. Applies to all expression variants including literals,
/// variables, binary ops, ternaries, and calls. Retains evaluation of effectful
/// subexpressions even when their result is unused.
pub(crate) fn prune_expr(expr: Expr) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::StringLiteral(value) => ExprKind::StringLiteral(value),
        ExprKind::IntLiteral(value) => ExprKind::IntLiteral(value),
        ExprKind::FloatLiteral(value) => ExprKind::FloatLiteral(value),
        ExprKind::Variable(name) => ExprKind::Variable(name),
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(prune_expr(*left)),
            op,
            right: Box::new(prune_expr(*right)),
        },
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(prune_expr(*value)),
            target: prune_instanceof_target(target),
        },
        ExprKind::BoolLiteral(value) => ExprKind::BoolLiteral(value),
        ExprKind::Null => ExprKind::Null,
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(prune_expr(*inner))),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(prune_expr(*inner))),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(prune_expr(*inner))),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(prune_expr(*inner))),
        ExprKind::ErrorSuppress(inner) => ExprKind::ErrorSuppress(Box::new(prune_expr(*inner))),
        ExprKind::Print(inner) => ExprKind::Print(Box::new(prune_expr(*inner))),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(prune_expr(*value)),
            default: Box::new(prune_expr(*default)),
        },
        ExprKind::Pipe { value, callable } => ExprKind::Pipe {
            value: Box::new(prune_expr(*value)),
            callable: Box::new(prune_expr(*callable)),
        },
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => ExprKind::Assignment {
            target: Box::new(prune_expr(*target)),
            value: Box::new(prune_expr(*value)),
            result_target: result_target.map(|target| Box::new(prune_expr(*target))),
            prelude: prelude.into_iter().flat_map(prune_stmt).collect(),
            conditional_value_temp,
        },
        ExprKind::PreIncrement(name) => ExprKind::PreIncrement(name),
        ExprKind::PostIncrement(name) => ExprKind::PostIncrement(name),
        ExprKind::PreDecrement(name) => ExprKind::PreDecrement(name),
        ExprKind::PostDecrement(name) => ExprKind::PostDecrement(name),
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::ArrayLiteral(items) => {
            ExprKind::ArrayLiteral(items.into_iter().map(prune_expr).collect())
        }
        ExprKind::ArrayLiteralAssoc(items) => ExprKind::ArrayLiteralAssoc(
            items.into_iter()
                .map(|(key, value)| (prune_expr(key), prune_expr(value)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            let subject = prune_expr(*subject);
            let arms: Vec<(Vec<Expr>, Expr)> = arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns.into_iter().map(prune_expr).collect(),
                        prune_expr(value),
                    )
                })
                .collect();
            let default = default.map(|expr| Box::new(prune_expr(*expr)));
            try_prune_match_expr(subject, arms, default)
        }
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(prune_expr(*array)),
            index: Box::new(prune_expr(*index)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(prune_expr(*condition)),
            then_expr: Box::new(prune_expr(*then_expr)),
            else_expr: Box::new(prune_expr(*else_expr)),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(prune_expr(*value)),
            default: Box::new(prune_expr(*default)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(prune_expr(*expr)),
        },
        ExprKind::Closure {
            params,
            variadic,
            return_type,
            body,
            is_arrow,
            is_static,
            captures,
            capture_refs,
        } => ExprKind::Closure {
            params,
            variadic,
            return_type,
            body: prune_block(body),
            is_arrow,
            is_static,
            captures,
            capture_refs,
        },
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(prune_expr(*value)),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(prune_expr(*inner))),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(prune_expr(*callee)),
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::ConstRef(name) => ExprKind::ConstRef(name),
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::NewDynamic { name_expr, args } => ExprKind::NewDynamic {
            name_expr: Box::new(prune_expr(*name_expr)),
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => ExprKind::NewDynamicObject {
            class_name: Box::new(prune_expr(*class_name)),
            fallback_class,
            required_parent,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(prune_expr(*object)),
            property,
        },
        ExprKind::DynamicPropertyAccess { object, property } => {
            ExprKind::DynamicPropertyAccess {
                object: Box::new(prune_expr(*object)),
                property: Box::new(prune_expr(*property)),
            }
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(prune_expr(*object)),
                property,
            }
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            ExprKind::NullsafeDynamicPropertyAccess {
                object: Box::new(prune_expr(*object)),
                property: Box::new(prune_expr(*property)),
            }
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            ExprKind::StaticPropertyAccess { receiver, property }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
            object: Box::new(prune_expr(*object)),
            method,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeMethodCall {
            object: Box::new(prune_expr(*object)),
            method,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(prune_callable_target(target))
        }
        ExprKind::This => ExprKind::This,
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(prune_expr(*expr)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(prune_expr(*len)),
        },
        ExprKind::ClassConstant { receiver } => ExprKind::ClassConstant { receiver },
        ExprKind::ScopedConstantAccess { receiver, name } => {
            ExprKind::ScopedConstantAccess { receiver, name }
        }
        ExprKind::NewScopedObject { receiver, args } => ExprKind::NewScopedObject {
            receiver,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::Yield { key, value } => ExprKind::Yield {
            key: key.map(|k| Box::new(prune_expr(*k))),
            value: value.map(|v| Box::new(prune_expr(*v))),
        },
        ExprKind::YieldFrom(inner) => ExprKind::YieldFrom(Box::new(prune_expr(*inner))),
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before optimizer passes")
        }
    };
    let kind = prune_unused_pure_subexpressions(kind);
    Expr { kind, span }
}

/// Lowers an instanceof target expression or name, applying prune_expr recursively
/// to any expression variant.
fn prune_instanceof_target(target: InstanceOfTarget) -> InstanceOfTarget {
    match target {
        InstanceOfTarget::Name(name) => InstanceOfTarget::Name(name),
        InstanceOfTarget::Expr(expr) => InstanceOfTarget::Expr(Box::new(prune_expr(*expr))),
    }
}

/// Returns true if the expression has observable side effects (function calls,
/// assignments, increments, prints, throws, etc.). Used to decide whether an
/// expression must be retained even when its result is unused.
pub(crate) fn expr_has_side_effects(expr: &Expr) -> bool {
    expr_effect(expr).has_side_effects
}

/// Returns the effect summary for a callable target. Static methods and plain
/// functions are treated as PURE; instance methods carry the effect of the
/// receiver expression.
pub(crate) fn callable_target_effect(target: &CallableTarget) -> Effect {
    match target {
        CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => Effect::PURE,
        CallableTarget::Method { object, .. } => expr_effect(object),
    }
}

/// Eliminates pure/constant subexpressions in ternary, short-ternary, null coalesce,
/// and short-circuit AND/OR expressions when the unused branch has no side effects.
/// For example, `$cond ? $a : $b` becomes just `$a` when `cond` is known truthy and
/// `$b` has no side effects.
pub(crate) fn prune_unused_pure_subexpressions(kind: ExprKind) -> ExprKind {
    match kind {
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => match scalar_value(&condition) {
            Some(value) if value.truthy() && !expr_has_side_effects(&else_expr) => then_expr.kind,
            Some(value) if !value.truthy() && !expr_has_side_effects(&then_expr) => else_expr.kind,
            _ => ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            },
        },
        ExprKind::ShortTernary { value, default } => match scalar_value(&value) {
            Some(value_scalar) if value_scalar.truthy() && !expr_has_side_effects(&default) => {
                value.kind
            }
            Some(value_scalar) if !value_scalar.truthy() => default.kind,
            _ => ExprKind::ShortTernary { value, default },
        },
        ExprKind::NullCoalesce { value, default } => match scalar_value(&value) {
            Some(ScalarValue::Null) => default.kind,
            Some(_) if !expr_has_side_effects(&default) => value.kind,
            _ => ExprKind::NullCoalesce { value, default },
        },
        ExprKind::BinaryOp { left, op, right } => match op {
            BinOp::And => match scalar_value(&left) {
                Some(value) if !value.truthy() && !expr_has_side_effects(&right) => {
                    ExprKind::BoolLiteral(false)
                }
                _ => ExprKind::BinaryOp { left, op, right },
            },
            BinOp::Or => match scalar_value(&left) {
                Some(value) if value.truthy() && !expr_has_side_effects(&right) => {
                    ExprKind::BoolLiteral(true)
                }
                _ => ExprKind::BinaryOp { left, op, right },
            },
            _ => ExprKind::BinaryOp { left, op, right },
        },
        other => other,
    }
}

/// Rewrites a callable target by recursively pruning the receiver/object expression
/// while preserving function names, static method receivers, and method names unchanged.
pub(crate) fn prune_callable_target(target: CallableTarget) -> CallableTarget {
    match target {
        CallableTarget::Function(name) => CallableTarget::Function(name),
        CallableTarget::StaticMethod { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(prune_expr(*object)),
            method,
        },
    }
}
