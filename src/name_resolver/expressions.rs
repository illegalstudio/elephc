use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver};

use super::names::{
    resolve_constant_name, resolve_function_name, resolve_special_or_class_name, resolve_type_expr,
};
use super::statements::{resolve_params, resolve_stmt_list};
use super::{resolved_name, rewrite_callback_literal_args, Imports, Symbols};

pub(super) fn resolve_expr(
    expr: &Expr,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Expr {
    let kind = match &expr.kind {
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(resolve_expr(left, current_namespace, imports, symbols)),
            op: op.clone(),
            right: Box::new(resolve_expr(right, current_namespace, imports, symbols)),
        },
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(resolve_expr(value, current_namespace, imports, symbols)),
            target: resolved_name(resolve_special_or_class_name(
                target,
                current_namespace,
                imports,
            )),
        },
        ExprKind::Throw(inner) => {
            ExprKind::Throw(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(resolve_expr(value, current_namespace, imports, symbols)),
            default: Box::new(resolve_expr(default, current_namespace, imports, symbols)),
        },
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name: resolved_name(resolve_function_name(
                name,
                current_namespace,
                imports,
                symbols,
            )),
            args: rewrite_callback_literal_args(
                name.as_str(),
                args,
                current_namespace,
                imports,
                symbols,
            )
            .into_iter()
            .map(|arg| resolve_expr(&arg, current_namespace, imports, symbols))
            .collect(),
        },
        ExprKind::ArrayLiteral(values) => ExprKind::ArrayLiteral(
            values
                .iter()
                .map(|value| resolve_expr(value, current_namespace, imports, symbols))
                .collect(),
        ),
        ExprKind::ArrayLiteralAssoc(values) => ExprKind::ArrayLiteralAssoc(
            values
                .iter()
                .map(|(key, value)| {
                    (
                        resolve_expr(key, current_namespace, imports, symbols),
                        resolve_expr(value, current_namespace, imports, symbols),
                    )
                })
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(resolve_expr(subject, current_namespace, imports, symbols)),
            arms: arms
                .iter()
                .map(|(conds, value)| {
                    (
                        conds
                            .iter()
                            .map(|cond| resolve_expr(cond, current_namespace, imports, symbols))
                            .collect(),
                        resolve_expr(value, current_namespace, imports, symbols),
                    )
                })
                .collect(),
            default: default
                .as_ref()
                .map(|expr| Box::new(resolve_expr(expr, current_namespace, imports, symbols))),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(resolve_expr(array, current_namespace, imports, symbols)),
            index: Box::new(resolve_expr(index, current_namespace, imports, symbols)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(resolve_expr(condition, current_namespace, imports, symbols)),
            then_expr: Box::new(resolve_expr(then_expr, current_namespace, imports, symbols)),
            else_expr: Box::new(resolve_expr(else_expr, current_namespace, imports, symbols)),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(resolve_expr(value, current_namespace, imports, symbols)),
            default: Box::new(resolve_expr(default, current_namespace, imports, symbols)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target: target.clone(),
            expr: Box::new(resolve_expr(expr, current_namespace, imports, symbols)),
        },
        ExprKind::Closure {
            params,
            variadic,
            body,
            is_arrow,
            captures,
        } => ExprKind::Closure {
            params: resolve_params(params, current_namespace, imports, symbols),
            variadic: variadic.clone(),
            body: resolve_stmt_list(body, current_namespace, imports, symbols)
                .expect("name resolver bug: closure body resolution failed"),
            is_arrow: *is_arrow,
            captures: captures.clone(),
        },
        ExprKind::Spread(inner) => {
            ExprKind::Spread(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var: var.clone(),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(resolve_expr(callee, current_namespace, imports, symbols)),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::ConstRef(name) => ExprKind::ConstRef(resolved_name(resolve_constant_name(
            name,
            current_namespace,
            imports,
            symbols,
        ))),
        ExprKind::EnumCase {
            enum_name,
            case_name,
        } => ExprKind::EnumCase {
            enum_name: resolved_name(resolve_special_or_class_name(
                enum_name,
                current_namespace,
                imports,
            )),
            case_name: case_name.clone(),
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name: resolved_name(resolve_special_or_class_name(
                class_name,
                current_namespace,
                imports,
            )),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
            property: property.clone(),
        },
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
                property: property.clone(),
            }
        }
        ExprKind::StaticPropertyAccess { receiver, property } => ExprKind::StaticPropertyAccess {
            receiver: match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolve_special_or_class_name(name, current_namespace, imports),
                )),
                _ => receiver.clone(),
            },
            property: property.clone(),
        },
        ExprKind::MethodCall { object, method, args } => ExprKind::MethodCall {
            object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
            method: method.clone(),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::NullsafeMethodCall { object, method, args } => ExprKind::NullsafeMethodCall {
            object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
            method: method.clone(),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver: match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolve_special_or_class_name(name, current_namespace, imports),
                )),
                _ => receiver.clone(),
            },
            method: method.clone(),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::FirstClassCallable(target) => ExprKind::FirstClassCallable(match target {
            CallableTarget::Function(name) => CallableTarget::Function(resolved_name(
                resolve_function_name(name, current_namespace, imports, symbols),
            )),
            CallableTarget::StaticMethod { receiver, method } => CallableTarget::StaticMethod {
                receiver: match receiver {
                    StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                        resolve_special_or_class_name(name, current_namespace, imports),
                    )),
                    _ => receiver.clone(),
                },
                method: method.clone(),
            },
            CallableTarget::Method { object, method } => CallableTarget::Method {
                object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
                method: method.clone(),
            },
        }),
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type: target_type.clone(),
            expr: Box::new(resolve_expr(expr, current_namespace, imports, symbols)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type: resolve_type_expr(element_type, current_namespace, imports),
            len: Box::new(resolve_expr(len, current_namespace, imports, symbols)),
        },
        _ => expr.kind.clone(),
    };
    Expr::new(kind, expr.span)
}
