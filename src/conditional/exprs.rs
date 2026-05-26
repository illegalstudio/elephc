//! Purpose:
//! Rewrites expressions while applying compiler conditional branches.
//! Recurses into nested statement bodies embedded in expression forms such as closures and preludes.
//!
//! Called from:
//! - `crate::conditional::stmts::apply_stmts()`.
//!
//! Key details:
//! - Expression shape and spans are preserved except where nested conditional statement lists are pruned.

use std::collections::HashSet;

use crate::parser::ast::{CallableTarget, Expr, ExprKind};

use super::stmts::apply_stmts;

/// Recursively rewrites an expression, applying compiler conditional branches for any
/// embedded statement bodies (e.g., closure bodies, assignment preludes).
///
/// For each `ExprKind` variant, recurses into child expressions. Closure bodies are
/// rewritten via `apply_stmts()` rather than `rewrite_expr()` so that nested `ifdef`
/// blocks inside the closure are handled correctly.
///
/// Constants (`ConstRef`), static property accesses, and simple name variants are
/// returned unchanged — ifdef symbols affect runtime code, not compile-time constants.
///
/// - Input: `expr` to rewrite, `defines` set of active `ifdef` symbols.
/// - Output: New `Expr` with conditionals applied, span preserved.
pub(super) fn rewrite_expr(expr: Expr, defines: &HashSet<String>) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(rewrite_expr(*left, defines)),
            op,
            right: Box::new(rewrite_expr(*right, defines)),
        },
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(rewrite_expr(*inner, defines))),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(rewrite_expr(*inner, defines))),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(rewrite_expr(*inner, defines))),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(rewrite_expr(*inner, defines))),
        ExprKind::ErrorSuppress(inner) => {
            ExprKind::ErrorSuppress(Box::new(rewrite_expr(*inner, defines)))
        }
        ExprKind::Print(inner) => ExprKind::Print(Box::new(rewrite_expr(*inner, defines))),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(rewrite_expr(*value, defines)),
            default: Box::new(rewrite_expr(*default, defines)),
        },
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => ExprKind::Assignment {
            target: Box::new(rewrite_expr(*target, defines)),
            value: Box::new(rewrite_expr(*value, defines)),
            result_target: result_target.map(|target| Box::new(rewrite_expr(*target, defines))),
            prelude: apply_stmts(prelude, defines),
            conditional_value_temp,
        },
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::ArrayLiteral(elems) => ExprKind::ArrayLiteral(
            elems
                .into_iter()
                .map(|elem| rewrite_expr(elem, defines))
                .collect(),
        ),
        ExprKind::ArrayLiteralAssoc(entries) => ExprKind::ArrayLiteralAssoc(
            entries
                .into_iter()
                .map(|(key, value)| (rewrite_expr(key, defines), rewrite_expr(value, defines)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(rewrite_expr(*subject, defines)),
            arms: arms
                .into_iter()
                .map(|(values, expr)| {
                    (
                        values
                            .into_iter()
                            .map(|value| rewrite_expr(value, defines))
                            .collect(),
                        rewrite_expr(expr, defines),
                    )
                })
                .collect(),
            default: default.map(|expr| Box::new(rewrite_expr(*expr, defines))),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(rewrite_expr(*array, defines)),
            index: Box::new(rewrite_expr(*index, defines)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(rewrite_expr(*condition, defines)),
            then_expr: Box::new(rewrite_expr(*then_expr, defines)),
            else_expr: Box::new(rewrite_expr(*else_expr, defines)),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(rewrite_expr(*value, defines)),
            default: Box::new(rewrite_expr(*default, defines)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(rewrite_expr(*expr, defines)),
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
            params: params
                .into_iter()
                .map(|(name, type_ann, default, is_ref)| {
                    (name, type_ann, default.map(|expr| rewrite_expr(expr, defines)), is_ref)
                })
                .collect(),
            variadic,
            return_type,
            body: apply_stmts(body, defines),
            is_arrow,
            is_static,
            captures,
            capture_refs,
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(rewrite_expr(*inner, defines))),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(rewrite_expr(*callee, defines)),
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::ConstRef(name) => ExprKind::ConstRef(name),
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(rewrite_expr(*object, defines)),
            property,
        },
        ExprKind::DynamicPropertyAccess { object, property } => {
            ExprKind::DynamicPropertyAccess {
                object: Box::new(rewrite_expr(*object, defines)),
                property: Box::new(rewrite_expr(*property, defines)),
            }
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(rewrite_expr(*object, defines)),
                property,
            }
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            ExprKind::NullsafeDynamicPropertyAccess {
                object: Box::new(rewrite_expr(*object, defines)),
                property: Box::new(rewrite_expr(*property, defines)),
            }
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            ExprKind::StaticPropertyAccess { receiver, property }
        }
        ExprKind::MethodCall { object, method, args } => ExprKind::MethodCall {
            object: Box::new(rewrite_expr(*object, defines)),
            method,
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeMethodCall {
            object: Box::new(rewrite_expr(*object, defines)),
            method,
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: args
                .into_iter()
                .map(|arg| rewrite_expr(arg, defines))
                .collect(),
        },
        ExprKind::FirstClassCallable(target) => ExprKind::FirstClassCallable(match target {
            CallableTarget::Function(name) => CallableTarget::Function(name),
            CallableTarget::StaticMethod { receiver, method } => {
                CallableTarget::StaticMethod { receiver, method }
            }
            CallableTarget::Method { object, method } => CallableTarget::Method {
                object: Box::new(rewrite_expr(*object, defines)),
                method,
            },
        }),
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(rewrite_expr(*expr, defines)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(rewrite_expr(*len, defines)),
        },
        other => other,
    };
    Expr::new(kind, span)
}
