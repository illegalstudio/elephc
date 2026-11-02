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
