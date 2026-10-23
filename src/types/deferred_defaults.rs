//! Purpose:
//! Shares normalized default-error queries and source constant names across compiler consumers.
//!
//! Called from:
//! - Method exception analysis, omitted-argument ownership and native default helper generation.
//! - Native and eval reflection parameter-default metadata.
//!
//! Key details:
//! - Finding a possible error is distinct from selecting an unconditional property error.
//! - Lazy branches retain their expression structure rather than becoming eager root errors.

use crate::parser::ast::{BinOp, Expr, ExprKind};

/// Preserves a declared class-constant spelling independently of its normalized value or Error.
pub(crate) fn source_constant_name(default: &Expr) -> Option<String> {
    use crate::parser::ast::StaticReceiver;
    let ExprKind::ScopedConstantAccess { receiver, name } = &default.kind else { return None; };
    let receiver = match receiver.written_class_receiver() {
        StaticReceiver::Named(name) => name.as_str().trim_start_matches('\\').to_string(),
        StaticReceiver::Self_ => "self".into(),
        StaticReceiver::Parent => "parent".into(),
        StaticReceiver::Static => "static".into(),
        StaticReceiver::Generic(_) => return None,
    };
    Some(format!("{receiver}::{name}"))
}

/// Returns whether any node of a SOURCE expression is a `throw`.
///
/// PHP forbids `throw` in a constant expression. The checker builds its deferred lexical
/// `Error` as a `Throw` too, but only into a normalized copy: callers that still hold the
/// source expression use this query to reject a source-written throw on its own.
///
/// The walk visits every `ExprKind` that can carry a sub-expression, because a throw may sit
/// anywhere PHP rejects it (`@(throw ...)`, a call argument, an array element, a match arm).
/// Closure and arrow-function bodies are deliberately not visited: their throw runs when the
/// closure is called, not when the constant is evaluated.
pub(crate) fn contains_throw(expr: &Expr) -> bool {
    use crate::parser::ast::{CallableTarget, InstanceOfTarget};
    match &expr.kind {
        ExprKind::Throw(_) => true,
        ExprKind::StringLiteral(_) | ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_) | ExprKind::Null | ExprKind::Variable(_)
        | ExprKind::ConstRef(_) | ExprKind::This | ExprKind::MagicConstant(_)
        | ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::StaticPropertyAccess { .. } | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_) | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_) => false,
        // A closure body is deferred until the closure runs; its defaults are not.
        ExprKind::Closure { params, .. } => params
            .iter()
            .any(|(_, _, default, _)| default.as_ref().is_some_and(contains_throw)),
        ExprKind::Negate(inner) | ExprKind::Not(inner) | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner) | ExprKind::Print(inner)
        | ExprKind::Clone(inner) | ExprKind::Spread(inner) | ExprKind::YieldFrom(inner)
        | ExprKind::PtrCast { expr: inner, .. } | ExprKind::Cast { expr: inner, .. }
        | ExprKind::NamedArg { value: inner, .. } | ExprKind::IncludeValue { path: inner, .. }
        | ExprKind::ObjectClassName { object: inner }
        | ExprKind::PropertyAccess { object: inner, .. }
        | ExprKind::NullsafePropertyAccess { object: inner, .. } => contains_throw(inner),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            contains_throw(object) || contains_throw(property)
        }
        ExprKind::BufferNew { len, .. } => contains_throw(len),
        ExprKind::BinaryOp { left, right, .. } => contains_throw(left) || contains_throw(right),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default }
        | ExprKind::Pipe { value, callable: default } => {
            contains_throw(value) || contains_throw(default)
        }
        ExprKind::Assignment { target, value, result_target, .. } => {
            contains_throw(target)
                || contains_throw(value)
                || result_target.as_deref().is_some_and(contains_throw)
        }
        ExprKind::InstanceOf { value, target } => {
            contains_throw(value)
                || match target {
                    InstanceOfTarget::Expr(inner) => contains_throw(inner),
                    InstanceOfTarget::Name(_) | InstanceOfTarget::Generic(_) => false,
                }
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(contains_throw),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| contains_throw(key) || contains_throw(value)),
        ExprKind::ArrayLiteralMixed(entries) => entries
            .iter()
            .any(|entry| entry.exprs().any(contains_throw)),
        ExprKind::Match { subject, arms, default } => {
            contains_throw(subject)
                || arms.iter().any(|(conditions, value)| {
                    conditions.iter().any(contains_throw) || contains_throw(value)
                })
                || default.as_deref().is_some_and(contains_throw)
        }
        ExprKind::ArrayAccess { array, index } => contains_throw(array) || contains_throw(index),
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            contains_throw(condition) || contains_throw(then_expr) || contains_throw(else_expr)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewGeneric { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::ClosureCall { args, .. } => args.iter().any(contains_throw),
        ExprKind::ExprCall { callee, args } => {
            contains_throw(callee) || args.iter().any(contains_throw)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            contains_throw(name_expr) || args.iter().any(contains_throw)
        }
        ExprKind::NewDynamicObject { class_name, args, .. } => {
            contains_throw(class_name) || args.iter().any(contains_throw)
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            contains_throw(object) || args.iter().any(contains_throw)
        }
        ExprKind::NullsafeDynamicMethodCall { object, method, args } => {
            contains_throw(object) || contains_throw(method) || args.iter().any(contains_throw)
        }
        ExprKind::FirstClassCallable(target) => match target {
            CallableTarget::Method { object, .. } => contains_throw(object),
            CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => false,
        },
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(contains_throw)
                || value.as_deref().is_some_and(contains_throw)
        }
    }
}

/// Returns whether any normalized default node can raise a deferred lexical Error.
pub(crate) fn contains_deferred_error(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Throw(_) => true,
        ExprKind::ArrayLiteral(values) | ExprKind::NewObject { args: values, .. }
        | ExprKind::NewScopedObject { args: values, .. } =>
            values.iter().any(contains_deferred_error),
        ExprKind::ArrayLiteralAssoc(entries) => entries.iter()
            .any(|(key, value)| contains_deferred_error(key) || contains_deferred_error(value)),
        ExprKind::ArrayLiteralMixed(entries) => entries.iter()
            .any(|entry| entry.exprs().any(contains_deferred_error)),
        ExprKind::BinaryOp { left, right, .. } =>
            contains_deferred_error(left) || contains_deferred_error(right),
        ExprKind::ArrayAccess { array, index } =>
            contains_deferred_error(array) || contains_deferred_error(index),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } =>
            contains_deferred_error(value) || contains_deferred_error(default),
        ExprKind::Ternary { condition, then_expr, else_expr } =>
            contains_deferred_error(condition) || contains_deferred_error(then_expr)
                || contains_deferred_error(else_expr),
        ExprKind::Match { subject, arms, default } => contains_deferred_error(subject)
            || arms.iter().any(|(conditions, result)|
                conditions.iter().any(contains_deferred_error) || contains_deferred_error(result))
            || default.as_deref().is_some_and(contains_deferred_error),
        ExprKind::Negate(inner) | ExprKind::Not(inner) | ExprKind::BitNot(inner)
        | ExprKind::NamedArg { value: inner, .. } | ExprKind::Spread(inner)
        | ExprKind::Cast { expr: inner, .. } =>
            contains_deferred_error(inner),
        _ => false,
    }
}

/// Extracts a compiler-generated lazy Error message without evaluating its expression.
pub(crate) fn deferred_error_message(default: &Expr) -> Option<&str> {
    let ExprKind::Throw(exception) = &default.kind else { return None; };
    let ExprKind::NewObject { class_name, args } = &exception.kind else { return None; };
    let [Expr { kind: ExprKind::StringLiteral(message), .. }] = args.as_slice() else { return None; };
    (class_name.as_str() == "Error").then_some(message.as_str())
}

/// Finds a necessarily evaluated error while respecting literal lazy property branches.
pub(crate) fn unconditional_error_message(default: &Expr) -> Option<&str> {
    if let Some(message) = deferred_error_message(default) { return Some(message); }
    match &default.kind {
        ExprKind::ArrayLiteral(items) => items.iter().find_map(unconditional_error_message),
        ExprKind::ArrayLiteralAssoc(items) => items.iter().find_map(|(key, value)|
            unconditional_error_message(key).or_else(|| unconditional_error_message(value))),
        ExprKind::ArrayLiteralMixed(items) => items.iter()
            .find_map(|item| item.exprs().find_map(unconditional_error_message)),
        ExprKind::BinaryOp { left, op, right } => unconditional_error_message(left).or_else(|| {
            let evaluates_right = match op {
                BinOp::And => literal_truthiness(left) == Some(true),
                BinOp::Or => literal_truthiness(left) == Some(false),
                BinOp::NullCoalesce => matches!(left.kind, ExprKind::Null),
                _ => true,
            };
            evaluates_right.then(|| unconditional_error_message(right)).flatten()
        }),
        ExprKind::ArrayAccess { array, index } => unconditional_error_message(array)
            .or_else(|| unconditional_error_message(index)),
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            unconditional_error_message(condition).or_else(|| {
                let selected = if literal_truthiness(condition)? { then_expr } else { else_expr };
                unconditional_error_message(selected)
            })
        }
        ExprKind::NullCoalesce { value, default } => {
            unconditional_error_message(value).or_else(|| {
                matches!(value.kind, ExprKind::Null).then(|| unconditional_error_message(default)).flatten()
            })
        }
        ExprKind::ShortTernary { value, default } => unconditional_error_message(value)
            .or_else(|| (!literal_truthiness(value)?).then(|| unconditional_error_message(default)).flatten()),
        ExprKind::Negate(inner) | ExprKind::Not(inner) | ExprKind::BitNot(inner)
        | ExprKind::Cast { expr: inner, .. } | ExprKind::Spread(inner) =>
            unconditional_error_message(inner),
        _ => None,
    }
}

/// Resolves PHP truthiness only for side-effect-free literal branch selectors.
fn literal_truthiness(value: &Expr) -> Option<bool> {
    match &value.kind {
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::Null => Some(false),
        ExprKind::IntLiteral(value) => Some(*value != 0),
        ExprKind::FloatLiteral(value) => Some(*value != 0.0),
        ExprKind::StringLiteral(value) => Some(!value.is_empty() && value != "0"),
        ExprKind::ArrayLiteral(items) if items.iter().all(|item| literal_truthiness(item).is_some()) =>
            Some(!items.is_empty()),
        _ => None,
    }
}

/// Removes only statically selected lazy branches after every hard default error was validated.
pub(crate) fn prune_literal_branches(expr: &mut Expr) {
    match &mut expr.kind {
        ExprKind::ArrayLiteral(items) | ExprKind::NewObject { args: items, .. }
        | ExprKind::NewScopedObject { args: items, .. } => {
            for value in items { prune_literal_branches(value); }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                prune_literal_branches(key); prune_literal_branches(value);
            }
        }
        ExprKind::ArrayLiteralMixed(items) => {
            for item in items { for value in item.exprs_mut() { prune_literal_branches(value); } }
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            prune_literal_branches(condition);
            prune_literal_branches(then_expr);
            prune_literal_branches(else_expr);
            if let Some(truthy) = literal_truthiness(condition) {
                *expr = if truthy { *then_expr.clone() } else { *else_expr.clone() };
            }
        }
        ExprKind::NullCoalesce { value, default } => {
            prune_literal_branches(value); prune_literal_branches(default);
            if matches!(value.kind, ExprKind::Null) {
                *expr = *default.clone();
            } else if literal_truthiness(value).is_some() {
                *expr = *value.clone();
            }
        }
        ExprKind::ShortTernary { value, default } => {
            prune_literal_branches(value); prune_literal_branches(default);
            if let Some(truthy) = literal_truthiness(value) {
                *expr = if truthy { *value.clone() } else { *default.clone() };
            }
        }
        ExprKind::BinaryOp { left, op, right } => {
            prune_literal_branches(left); prune_literal_branches(right);
            match op {
                BinOp::And if literal_truthiness(left) == Some(false) =>
                    *expr = Expr::new(ExprKind::BoolLiteral(false), expr.span),
                BinOp::Or if literal_truthiness(left) == Some(true) =>
                    *expr = Expr::new(ExprKind::BoolLiteral(true), expr.span),
                _ => {}
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            prune_literal_branches(array); prune_literal_branches(index);
        }
        ExprKind::Negate(inner) | ExprKind::Not(inner) | ExprKind::BitNot(inner)
        | ExprKind::NamedArg { value: inner, .. } | ExprKind::Spread(inner)
        | ExprKind::Cast { expr: inner, .. } =>
            prune_literal_branches(inner),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    /// Builds a literal node without irrelevant source positions.
    fn node(kind: ExprKind) -> Expr { Expr::new(kind, Span::dummy()) }

    /// Creates one normalized default Error as produced by lexical binding.
    fn error() -> Expr {
        node(ExprKind::Throw(Box::new(node(ExprKind::NewObject {
            class_name: crate::names::Name::unqualified("Error"),
            args: vec![node(ExprKind::StringLiteral("missing parent".into()))],
        }))))
    }

    /// Literal falsehood, including PHP's string zero, removes only the unused branch.
    #[test]
    fn deferred_default_literal_branch_pruning_preserves_selection() {
        for condition in [ExprKind::BoolLiteral(false), ExprKind::Null,
            ExprKind::StringLiteral("0".into()), ExprKind::ArrayLiteral(Vec::new())]
        {
            let mut expression = node(ExprKind::Ternary {
                condition: Box::new(node(condition)), then_expr: Box::new(error()),
                else_expr: Box::new(node(ExprKind::StringLiteral("ok".into()))),
            });
            assert!(contains_deferred_error(&expression));
            prune_literal_branches(&mut expression);
            assert_eq!(expression.kind, ExprKind::StringLiteral("ok".into()));
        }
    }

    /// A nonempty array selector still evaluates its throwing entries before choosing a branch.
    #[test]
    fn deferred_default_array_selector_retains_observable_evaluation() {
        let mut expression = node(ExprKind::Ternary {
            condition: Box::new(node(ExprKind::ArrayLiteral(vec![error()]))),
            then_expr: Box::new(node(ExprKind::StringLiteral("ok".into()))),
            else_expr: Box::new(node(ExprKind::Null)),
        });
        prune_literal_branches(&mut expression);
        assert!(matches!(expression.kind, ExprKind::Ternary { .. }));
        assert_eq!(unconditional_error_message(&expression), Some("missing parent"));
    }

    /// Mixed array entries and strict operands retain errors, but boolean short circuits skip them.
    #[test]
    fn deferred_default_error_queries_cover_mixed_arrays_and_short_circuits() {
        let mixed = node(ExprKind::ArrayLiteralMixed(vec![crate::parser::ast::ArrayEntry::Spread(
            node(ExprKind::ArrayLiteral(vec![error()])),
        )]));
        assert!(contains_deferred_error(&mixed));
        assert_eq!(unconditional_error_message(&mixed), Some("missing parent"));
        for (op, condition, expected) in [(BinOp::And, false, false), (BinOp::Or, true, true)] {
            let mut expression = node(ExprKind::BinaryOp {
                left: Box::new(node(ExprKind::BoolLiteral(condition))), op,
                right: Box::new(error()),
            });
            assert_eq!(unconditional_error_message(&expression), None);
            prune_literal_branches(&mut expression);
            assert_eq!(expression.kind, ExprKind::BoolLiteral(expected));
        }
    }
}
