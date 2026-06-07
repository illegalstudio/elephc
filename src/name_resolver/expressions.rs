//! Purpose:
//! Resolves names embedded in expressions and callable targets.
//! Rewrites function, constant, class, method, enum, object, and instanceof references as needed.
//!
//! Called from:
//! - `crate::name_resolver::statements` and declaration resolvers.
//!
//! Key details:
//! - PHP builtin fallback applies to unqualified function calls without breaking explicit namespace references.

use crate::names::php_symbol_key;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, InstanceOfTarget, StaticReceiver};
use crate::span::Span;

use super::names::{
    resolve_constant_name, resolve_function_name, resolve_special_or_class_name,
    resolve_type_expr, resolved_class_constant_name,
};
use super::statements::{resolve_params, resolve_stmt_list};
use super::{resolved_name, rewrite_callback_literal_args, Imports, Symbols};

/// Recursively resolves names in an expression, returning a new expression with
/// all name references rewritten according to namespace and import rules.
///
/// Handles function calls, class/constant references, instanceof targets, closures,
/// method calls, and all other expression variants. Unqualified names are resolved
/// against current_namespace and imports. PHP builtin fallback applies to function
/// names that remain unqualified after resolution.
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
            target: resolve_instanceof_target(target, current_namespace, imports, symbols),
        },
        ExprKind::Throw(inner) => {
            ExprKind::Throw(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::Print(inner) => {
            ExprKind::Print(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::Not(inner) => {
            ExprKind::Not(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::Negate(inner) => {
            ExprKind::Negate(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::BitNot(inner) => {
            ExprKind::BitNot(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::ErrorSuppress(inner) => ExprKind::ErrorSuppress(Box::new(resolve_expr(
            inner,
            current_namespace,
            imports,
            symbols,
        ))),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(resolve_expr(value, current_namespace, imports, symbols)),
            default: Box::new(resolve_expr(default, current_namespace, imports, symbols)),
        },
        ExprKind::Pipe { value, callable } => ExprKind::Pipe {
            value: Box::new(resolve_expr(value, current_namespace, imports, symbols)),
            callable: Box::new(resolve_expr(callable, current_namespace, imports, symbols)),
        },
        ExprKind::FunctionCall { name, args } => {
            let function_name = resolve_function_name(name, current_namespace, imports, symbols);
            let resolved_args: Vec<Expr> = rewrite_callback_literal_args(
                &function_name,
                args,
                current_namespace,
                imports,
                symbols,
            )
            .into_iter()
            .map(|arg| resolve_expr(&arg, current_namespace, imports, symbols))
            .collect();
            match fold_variadic_array_set_call(&function_name, &resolved_args, expr.span) {
                Some(folded) => folded,
                None => ExprKind::FunctionCall {
                    name: resolved_name(function_name.clone()),
                    args: resolved_args,
                },
            }
        }
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
            return_type,
            body,
            is_arrow,
            is_static,
            captures,
            capture_refs,
        } => ExprKind::Closure {
            params: resolve_params(params, current_namespace, imports, symbols),
            variadic: variadic.clone(),
            return_type: return_type
                .as_ref()
                .map(|ty| resolve_type_expr(ty, current_namespace, imports, symbols)),
            body: resolve_stmt_list(body, current_namespace, imports, symbols)
                .expect("name resolver bug: closure body resolution failed"),
            is_arrow: *is_arrow,
            is_static: *is_static,
            captures: captures.clone(),
            capture_refs: capture_refs.clone(),
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
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name: resolved_name(resolve_special_or_class_name(
                class_name,
                current_namespace,
                imports,
                symbols,
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
        ExprKind::DynamicPropertyAccess { object, property } => {
            ExprKind::DynamicPropertyAccess {
                object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
                property: Box::new(resolve_expr(property, current_namespace, imports, symbols)),
            }
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
                property: property.clone(),
            }
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            ExprKind::NullsafeDynamicPropertyAccess {
                object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
                property: Box::new(resolve_expr(property, current_namespace, imports, symbols)),
            }
        }
        ExprKind::StaticPropertyAccess { receiver, property } => ExprKind::StaticPropertyAccess {
            receiver: match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolve_special_or_class_name(name, current_namespace, imports, symbols),
                )),
                _ => receiver.clone(),
            },
            property: property.clone(),
        },
        ExprKind::ClassConstant { receiver } => ExprKind::ClassConstant {
            receiver: match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolved_class_constant_name(name, current_namespace, imports),
                )),
                _ => receiver.clone(),
            },
        },
        ExprKind::ScopedConstantAccess { receiver, name } => ExprKind::ScopedConstantAccess {
            receiver: match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolve_special_or_class_name(name, current_namespace, imports, symbols),
                )),
                _ => receiver.clone(),
            },
            name: name.clone(),
        },
        ExprKind::NewScopedObject { receiver, args } => ExprKind::NewScopedObject {
            receiver: match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolve_special_or_class_name(name, current_namespace, imports, symbols),
                )),
                _ => receiver.clone(),
            },
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
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
                    resolve_special_or_class_name(name, current_namespace, imports, symbols),
                )),
                _ => receiver.clone(),
            },
            method: php_symbol_key(method),
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
                        resolve_special_or_class_name(name, current_namespace, imports, symbols),
                    )),
                    _ => receiver.clone(),
                },
                method: php_symbol_key(method),
            },
            CallableTarget::Method { object, method } => CallableTarget::Method {
                object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
                method: php_symbol_key(method),
            },
        }),
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type: target_type.clone(),
            expr: Box::new(resolve_expr(expr, current_namespace, imports, symbols)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type: resolve_type_expr(element_type, current_namespace, imports, symbols),
            len: Box::new(resolve_expr(len, current_namespace, imports, symbols)),
        },
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => ExprKind::Assignment {
            target: Box::new(resolve_expr(target, current_namespace, imports, symbols)),
            value: Box::new(resolve_expr(value, current_namespace, imports, symbols)),
            result_target: result_target
                .as_ref()
                .map(|t| Box::new(resolve_expr(t, current_namespace, imports, symbols))),
            // `prelude` is empty at name-resolution time (assignment preludes are
            // synthesized later during codegen), so there is nothing to resolve here.
            prelude: prelude.clone(),
            conditional_value_temp: conditional_value_temp.clone(),
        },
        ExprKind::Yield { key, value } => ExprKind::Yield {
            key: key
                .as_ref()
                .map(|k| Box::new(resolve_expr(k, current_namespace, imports, symbols))),
            value: value
                .as_ref()
                .map(|v| Box::new(resolve_expr(v, current_namespace, imports, symbols))),
        },
        ExprKind::YieldFrom(inner) => ExprKind::YieldFrom(Box::new(resolve_expr(
            inner,
            current_namespace,
            imports,
            symbols,
        ))),
        ExprKind::NewDynamic { name_expr, args } => ExprKind::NewDynamic {
            name_expr: Box::new(resolve_expr(name_expr, current_namespace, imports, symbols)),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => ExprKind::NewDynamicObject {
            class_name: Box::new(resolve_expr(class_name, current_namespace, imports, symbols)),
            fallback_class: fallback_class.clone(),
            required_parent: required_parent.clone(),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        _ => expr.kind.clone(),
    };
    Expr::new(kind, expr.span)
}

/// Left-folds a variadic call to one of PHP's left-associative array set/merge builtins
/// (`array_merge`, `array_diff`, `array_intersect`, `array_diff_key`, `array_intersect_key`) with
/// more than two arguments into nested two-argument calls — e.g. `array_merge(a, b, c)` becomes
/// `array_merge(array_merge(a, b), c)`. Each of these builtins is left-associative
/// (`a ∪ b ∪ c`, `a \ (b ∪ c)`, `a ∩ b ∩ c`), so the rewrite is semantics-preserving and lets the
/// existing two-argument codegen handle the variadic forms without a dedicated N-ary runtime.
///
/// Returns `None` (leaving the call unchanged) when the function is not one of these builtins, when
/// there are two or fewer arguments, or when any argument is a spread (whose count is not known
/// statically). `args` must already be name-resolved; `span` is reused for the synthesized calls.
fn fold_variadic_array_set_call(
    function_name: &str,
    args: &[Expr],
    span: Span,
) -> Option<ExprKind> {
    const FOLDABLE: &[&str] = &[
        "array_merge",
        "array_diff",
        "array_intersect",
        "array_diff_key",
        "array_intersect_key",
    ];
    let key = php_symbol_key(function_name.trim_start_matches('\\'));
    if !FOLDABLE.iter().any(|candidate| php_symbol_key(candidate) == key) {
        return None;
    }
    if args.len() <= 2 || args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
        return None;
    }
    let mut folded = ExprKind::FunctionCall {
        name: resolved_name(function_name.to_string()),
        args: vec![args[0].clone(), args[1].clone()],
    };
    for arg in &args[2..] {
        folded = ExprKind::FunctionCall {
            name: resolved_name(function_name.to_string()),
            args: vec![Expr::new(folded, span), arg.clone()],
        };
    }
    Some(folded)
}

/// Resolves the target of an instanceof expression.
///
/// If the target is a bare name, it is rewritten using resolve_special_or_class_name
/// to apply namespace/use rules. Expression targets are recursively resolved.
fn resolve_instanceof_target(
    target: &InstanceOfTarget,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> InstanceOfTarget {
    match target {
        InstanceOfTarget::Name(name) => InstanceOfTarget::Name(resolved_name(
            resolve_special_or_class_name(name, current_namespace, imports, symbols),
        )),
        InstanceOfTarget::Expr(expr) => InstanceOfTarget::Expr(Box::new(resolve_expr(
            expr,
            current_namespace,
            imports,
            symbols,
        ))),
    }
}
