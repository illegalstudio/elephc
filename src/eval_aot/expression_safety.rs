//! Purpose:
//! Checks expressions, callbacks, and call_user_func forms for EIR AOT.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - Static callback normalization reuses the same target support predicates.

use super::*;

/// Checks one expression for the initial no-scope EIR-function eval subset.
pub(super) fn expr_is_eir_function_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::ArrayAppend => false,
        ExprKind::Variable(name) => {
            facts.is_assigned(name) || scope_reads.is_some_and(|reads| reads.contains(name))
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner) => expr_is_eir_function_safe(inner, support, facts, scope_reads),
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => facts.is_int_local(name),
        ExprKind::BinaryOp { left, op, right } => {
            matches!(
                op,
                BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Mod
                    | BinOp::Pow
                    | BinOp::Lt
                    | BinOp::Gt
                    | BinOp::LtEq
                    | BinOp::GtEq
                    | BinOp::Eq
                    | BinOp::NotEq
                    | BinOp::StrictEq
                    | BinOp::StrictNotEq
                    | BinOp::And
                    | BinOp::Or
                    | BinOp::Xor
                    | BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::ShiftLeft
                    | BinOp::ShiftRight
                    | BinOp::Spaceship
                    | BinOp::Concat
            ) && expr_is_eir_function_safe(left, support, facts, scope_reads)
                && expr_is_eir_function_safe(right, support, facts, scope_reads)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_is_eir_function_safe(condition, support, facts, scope_reads)
                && expr_is_eir_function_safe(then_expr, support, facts, scope_reads)
                && expr_is_eir_function_safe(else_expr, support, facts, scope_reads)
        }
        ExprKind::ShortTernary { value, default } => {
            expr_is_eir_function_safe(value, support, facts, scope_reads)
                && expr_is_eir_function_safe(default, support, facts, scope_reads)
        }
        ExprKind::NullCoalesce { value, default } => {
            expr_is_eir_function_safe(value, support, facts, scope_reads)
                && expr_is_eir_function_safe(default, support, facts, scope_reads)
        }
        ExprKind::Cast { target, expr } => {
            matches!(
                target,
                CastType::Int | CastType::Float | CastType::String | CastType::Bool
            ) && expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_is_eir_function_safe(subject, support, facts, scope_reads)
                && default.as_ref().is_some_and(|default| {
                    expr_is_eir_function_safe(default, support, facts, scope_reads)
                })
                && arms.iter().all(|(conditions, result)| {
                    conditions.iter().all(|condition| {
                        expr_is_eir_function_safe(condition, support, facts, scope_reads)
                    }) && expr_is_eir_function_safe(result, support, facts, scope_reads)
                })
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_is_eir_static_array_source_safe(array, support, facts, scope_reads)
                && expr_is_eir_function_safe(index, support, facts, scope_reads)
        }
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads)
        }
        ExprKind::FunctionCall { name, args } => {
            eir_call_user_func_call_is_safe(name.as_str(), args, support, facts, scope_reads)
                || eir_construct_call_is_safe(name.as_str(), args, support, facts, scope_reads)
                || eir_runtime_builtin_call_is_safe(
                    name.as_str(),
                    args,
                    support,
                    facts,
                    scope_reads,
                )
                || fold_static_builtin_int_call(name.as_str().trim_start_matches('\\'), args)
                    .is_some()
                || support.function_supported(name.as_str(), args)
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => support.static_method_supported(receiver, method, args),
        _ => false,
    }
}

/// Returns true when a static `call_user_func*()` callback maps to an AOT-safe call.
pub(super) fn eir_call_user_func_call_is_safe<S>(
    name: &str,
    args: &[Expr],
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "call_user_func" => {
            let Some((callback, callback_args)) = args.split_first() else {
                return false;
            };
            static_callback_call_is_eir_safe(callback, callback_args, support, facts, scope_reads)
        }
        "call_user_func_array" => {
            let [callback, arg_array] = args else {
                return false;
            };
            let Some(callback_args) = static_call_user_func_array_args(arg_array) else {
                return false;
            };
            static_callback_call_is_eir_safe(callback, &callback_args, support, facts, scope_reads)
        }
        _ => false,
    }
}

/// Returns true when a compile-time callback names a safe function or static method.
pub(super) fn static_callback_call_is_eir_safe<S>(
    callback: &Expr,
    callback_args: &[Expr],
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    if let Some((receiver, method)) = static_callback_static_method_parts(callback) {
        return support.static_method_supported(&receiver, method.as_str(), callback_args);
    }
    static_callback_function_name(callback).is_some_and(|callback_name| {
        let short_callback = callback_name.trim_start_matches('\\');
        eir_runtime_builtin_call_is_safe(short_callback, callback_args, support, facts, scope_reads)
            || fold_static_builtin_call(short_callback, callback_args).is_some()
            || support.function_supported(short_callback, callback_args)
    })
}

/// Returns the function name from a compile-time callback expression.
pub(super) fn static_callback_function_name(callback: &Expr) -> Option<&str> {
    match &callback.kind {
        ExprKind::StringLiteral(name) if !name.contains("::") => Some(name.as_str()),
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => Some(name.as_str()),
        _ => None,
    }
}

/// Returns the named receiver and method from a compile-time static-method callback.
pub(super) fn static_callback_static_method_parts(callback: &Expr) -> Option<(StaticReceiver, String)> {
    match &callback.kind {
        ExprKind::StringLiteral(name) => static_callback_static_method_string_parts(name),
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            Some((receiver.clone(), method.clone()))
        }
        ExprKind::ArrayLiteral(items) => static_callback_static_method_array_parts(items),
        _ => None,
    }
}

/// Splits a literal `Class::method` callback into its receiver and method.
pub(super) fn static_callback_static_method_string_parts(name: &str) -> Option<(StaticReceiver, String)> {
    let (class_name, method) = name.trim_start_matches('\\').rsplit_once("::")?;
    let receiver = static_callback_static_method_named_receiver(class_name)?;
    if method.is_empty() {
        return None;
    }
    Some((receiver, method.to_string()))
}

/// Extracts a literal `["Class", "method"]` callback target.
pub(super) fn static_callback_static_method_array_parts(items: &[Expr]) -> Option<(StaticReceiver, String)> {
    let [class_expr, method_expr] = items else {
        return None;
    };
    let receiver = static_callback_static_method_array_receiver(class_expr)?;
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    if method.is_empty() {
        return None;
    }
    Some((receiver, method.clone()))
}

/// Returns the static receiver from the class part of a callable array.
pub(super) fn static_callback_static_method_array_receiver(class_expr: &Expr) -> Option<StaticReceiver> {
    match &class_expr.kind {
        ExprKind::StringLiteral(class_name) => {
            static_callback_static_method_named_receiver(class_name)
        }
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Named(name),
        } => Some(StaticReceiver::Named(name.clone())),
        _ => None,
    }
}

/// Returns a named static receiver from a literal class name.
pub(super) fn static_callback_static_method_named_receiver(class_name: &str) -> Option<StaticReceiver> {
    let class_name = class_name.trim_start_matches('\\');
    if class_name.is_empty() {
        return None;
    }
    Some(StaticReceiver::Named(Name::from(class_name.to_string())))
}

/// Converts a static `call_user_func_array()` argument array into callback args.
pub(super) fn static_call_user_func_array_args(arg_array: &Expr) -> Option<Vec<Expr>> {
    match &arg_array.kind {
        ExprKind::ArrayLiteral(items) => Some(items.clone()),
        ExprKind::ArrayLiteralAssoc(pairs) => {
            static_call_user_func_array_assoc_args(pairs.as_slice())
        }
        _ => None,
    }
}

/// Converts literal associative callback arrays into positional or named callback args.
pub(super) fn static_call_user_func_array_assoc_args(pairs: &[(Expr, Expr)]) -> Option<Vec<Expr>> {
    let mut args = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        match &key.kind {
            ExprKind::StringLiteral(name) => {
                args.push(Expr::new(
                    ExprKind::NamedArg {
                        name: name.clone(),
                        value: Box::new(value.clone()),
                    },
                    value.span,
                ));
            }
            ExprKind::IntLiteral(_) => args.push(value.clone()),
            _ => return None,
        }
    }
    Some(args)
}
