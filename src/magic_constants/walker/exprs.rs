//! Purpose:
//! Walks expression AST nodes for magic-constant substitution passes.
//! Recurses through calls, literals, access forms, closures, match arms, assignments, and nested statements.
//!
//! Called from:
//! - `crate::magic_constants::walker::stmts` and member walkers.
//!
//! Key details:
//! - Expression traversal must cover every `ExprKind` so raw magic constants cannot reach later passes.

use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget};

use super::stmts::{walk_program, walk_stmt};
use super::Pass;

pub(super) fn walk_expr<P: Pass>(expr: Expr, pass: &mut P) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::MagicConstant(mc) => pass.transform_magic(span, mc),

        ExprKind::StringLiteral(value) => pass.transform_string(value),

        // Leaves with no Expr subtrees:
        kind @ (ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::Variable(_)
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::This
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)) => kind,

        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(walk_expr(*left, pass)),
            op,
            right: Box::new(walk_expr(*right, pass)),
        },
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(walk_expr(*value, pass)),
            target: walk_instanceof_target(target, pass),
        },
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(walk_expr(*inner, pass))),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(walk_expr(*inner, pass))),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(walk_expr(*inner, pass))),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(walk_expr(*inner, pass))),
        ExprKind::ErrorSuppress(inner) => ExprKind::ErrorSuppress(Box::new(walk_expr(*inner, pass))),
        ExprKind::Print(inner) => ExprKind::Print(Box::new(walk_expr(*inner, pass))),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(walk_expr(*value, pass)),
            default: Box::new(walk_expr(*default, pass)),
        },
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => ExprKind::Assignment {
            target: Box::new(walk_expr(*target, pass)),
            value: Box::new(walk_expr(*value, pass)),
            result_target: result_target.map(|target| Box::new(walk_expr(*target, pass))),
            prelude: prelude.into_iter().map(|stmt| walk_stmt(stmt, pass)).collect(),
            conditional_value_temp,
        },
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::ArrayLiteral(items) => {
            ExprKind::ArrayLiteral(items.into_iter().map(|i| walk_expr(i, pass)).collect())
        }
        ExprKind::ArrayLiteralAssoc(pairs) => ExprKind::ArrayLiteralAssoc(
            pairs
                .into_iter()
                .map(|(k, v)| (walk_expr(k, pass), walk_expr(v, pass)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(walk_expr(*subject, pass)),
            arms: arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns.into_iter().map(|p| walk_expr(p, pass)).collect(),
                        walk_expr(value, pass),
                    )
                })
                .collect(),
            default: default.map(|d| Box::new(walk_expr(*d, pass))),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(walk_expr(*array, pass)),
            index: Box::new(walk_expr(*index, pass)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(walk_expr(*condition, pass)),
            then_expr: Box::new(walk_expr(*then_expr, pass)),
            else_expr: Box::new(walk_expr(*else_expr, pass)),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(walk_expr(*value, pass)),
            default: Box::new(walk_expr(*default, pass)),
        },
        ExprKind::Cast { target, expr: inner } => ExprKind::Cast {
            target,
            expr: Box::new(walk_expr(*inner, pass)),
        },
        ExprKind::Closure {
            params,
            variadic,
            return_type,
            body,
            is_arrow,
            is_static,
            captures,
        } => {
            pass.enter_closure(span);
            let new_params = params
                .into_iter()
                .map(|(n, t, default, by_ref)| {
                    (n, t, default.map(|d| walk_expr(d, pass)), by_ref)
                })
                .collect();
            let new_body = walk_program(body, pass);
            pass.leave_closure();
            ExprKind::Closure {
                params: new_params,
                variadic,
                return_type,
                body: new_body,
                is_arrow,
                is_static,
                captures,
            }
        }
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(walk_expr(*value, pass)),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(walk_expr(*inner, pass))),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(walk_expr(*callee, pass)),
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(walk_expr(*object, pass)),
            property,
        },
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(walk_expr(*object, pass)),
                property,
            }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
            object: Box::new(walk_expr(*object, pass)),
            method,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeMethodCall {
            object: Box::new(walk_expr(*object, pass)),
            method,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::PtrCast { target_type, expr: inner } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(walk_expr(*inner, pass)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(walk_expr(*len, pass)),
        },
        ExprKind::ClassConstant { receiver } => ExprKind::ClassConstant { receiver },
        ExprKind::ScopedConstantAccess { receiver, name } => {
            ExprKind::ScopedConstantAccess { receiver, name }
        }
        ExprKind::NewScopedObject { receiver, args } => ExprKind::NewScopedObject {
            receiver,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::Yield { key, value } => ExprKind::Yield {
            key: key.map(|k| Box::new(walk_expr(*k, pass))),
            value: value.map(|v| Box::new(walk_expr(*v, pass))),
        },
        ExprKind::YieldFrom(inner) => ExprKind::YieldFrom(Box::new(walk_expr(*inner, pass))),
    };
    Expr { kind, span }
}

fn walk_instanceof_target<P: Pass>(
    target: InstanceOfTarget,
    pass: &mut P,
) -> InstanceOfTarget {
    match target {
        InstanceOfTarget::Name(name) => InstanceOfTarget::Name(name),
        InstanceOfTarget::Expr(expr) => {
            InstanceOfTarget::Expr(Box::new(walk_expr(*expr, pass)))
        }
    }
}
