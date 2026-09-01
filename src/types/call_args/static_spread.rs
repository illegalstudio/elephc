//! Purpose:
//! Expands static spread-array arguments into positional / named argument forms.
//! Recognizes PHP array-unpack entries that can be resolved at compile time before planning.
//!
//! Called from:
//! - `crate::types::call_args::planner`
//!
//! Key details:
//! - String keys become named arguments, numeric keys remain positional, and duplicate static keys follow PHP last-wins behavior.

use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;

/// Describes how explicit or statically-expanded arguments occupy a fixed parameter list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FixedParameterSlots {
    NoNamedArguments,
    Contiguous(usize),
    Missing(usize),
    Unknown(String),
    Duplicate(usize),
    DynamicSpread,
}

/// Tracks whether a spread element came from the original source, a statically-known
/// associative positional element, a statically-known named element, or a positional
/// element that was part of a static tail after `...unpack_named`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExpandedArgOrigin {
    Source,
    StaticAssocPositional,
    StaticAssocNamed,
    StaticTailPositional,
}

/// The result of expanding static spread arrays from the call site.
///
/// `args` contains the flattened list of positional/named expressions;
/// `origins` parallels `args` and marks where each element came from.
pub(crate) struct ExpandedStaticSpreadArgs {
    pub(crate) args: Vec<Expr>,
    pub(crate) origins: Vec<ExpandedArgOrigin>,
}

/// Returns `true` if `args` contains any `NamedArg` expressions or spread
/// expressions that expand to a static associative array with named keys.
pub(crate) fn has_named_args(args: &[Expr]) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::NamedArg { .. } => true,
        ExprKind::Spread(inner) => static_assoc_spread_has_named_args(inner),
        _ => false,
    })
}

/// Expands each `...$array` spread in `args` that refers to a static associative
/// array literal into separate `NamedArg` or positional expressions; returns the
/// flattened list. Non-static spreads are left as-is.
pub(crate) fn expand_static_assoc_spread_args(args: &[Expr]) -> Vec<Expr> {
    expand_static_assoc_spread_args_with_origins(args).args
}

/// Maps named arguments onto `parameter_names` and reports holes before the highest slot.
///
/// Static associative array literals are expanded first so their string keys obey the
/// same parameter mapping as explicit named arguments. Runtime spreads remain dynamic.
pub(crate) fn fixed_parameter_slots(
    args: &[Expr],
    parameter_names: &[&str],
) -> FixedParameterSlots {
    let expanded = expand_static_assoc_spread_args(args);
    if expanded
        .iter()
        .any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
    {
        return FixedParameterSlots::DynamicSpread;
    }
    if !expanded
        .iter()
        .any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
    {
        return FixedParameterSlots::NoNamedArguments;
    }

    let mut occupied = vec![false; parameter_names.len()];
    let mut positional_index = 0usize;
    for arg in &expanded {
        let parameter_index = match &arg.kind {
            ExprKind::NamedArg { name, .. } => {
                let Some(index) = parameter_names
                    .iter()
                    .position(|parameter_name| parameter_name == name)
                else {
                    return FixedParameterSlots::Unknown(name.clone());
                };
                index
            }
            _ => {
                let index = positional_index;
                positional_index += 1;
                index
            }
        };
        if parameter_index >= occupied.len() {
            continue;
        }
        if occupied[parameter_index] {
            return FixedParameterSlots::Duplicate(parameter_index);
        }
        occupied[parameter_index] = true;
    }

    let Some(last_occupied) = occupied.iter().rposition(|slot| *slot) else {
        return FixedParameterSlots::Contiguous(0);
    };
    if let Some(first_missing) = occupied[..=last_occupied].iter().position(|slot| !slot) {
        return FixedParameterSlots::Missing(first_missing);
    }
    FixedParameterSlots::Contiguous(last_occupied + 1)
}

/// Like `expand_static_assoc_spread_args` but also returns an `origins` vector
/// parallel to the expanded arguments, marking where each element originated.
pub(crate) fn expand_static_assoc_spread_args_with_origins(
    args: &[Expr],
) -> ExpandedStaticSpreadArgs {
    let mut expanded_args = Vec::with_capacity(args.len());
    let mut origins = Vec::with_capacity(args.len());

    for arg in args {
        if let ExprKind::Spread(inner) = &arg.kind {
            if let Some(mut spread_args) = expand_static_spread(inner, arg.span) {
                for (arg, origin) in spread_args.drain(..) {
                    expanded_args.push(arg);
                    origins.push(origin);
                }
                continue;
            }
        }
        expanded_args.push(arg.clone());
        origins.push(ExpandedArgOrigin::Source);
    }

    ExpandedStaticSpreadArgs {
        args: expanded_args,
        origins,
    }
}

/// Returns `true` if `expr` is an `ArrayLiteralAssoc` and any of its keys
/// resolve to a named (string) key in PHP array-unpack semantics.
fn static_assoc_spread_has_named_args(expr: &Expr) -> bool {
    let ExprKind::ArrayLiteralAssoc(pairs) = &expr.kind else {
        return false;
    };
    pairs.iter().any(|(key, _)| {
        matches!(static_assoc_spread_key(key), Some(StaticAssocSpreadKey::Named(_)))
    })
}

/// If `expr` is a static associative array literal, returns its key/value pairs
/// as expanded `NamedArg` and positional expressions. Otherwise returns `None`.
fn expand_static_spread(
    expr: &Expr,
    spread_span: Span,
) -> Option<Vec<(Expr, ExpandedArgOrigin)>> {
    match &expr.kind {
        ExprKind::ArrayLiteralAssoc(pairs) => expand_static_assoc_spread(pairs, spread_span),
        _ => None,
    }
}

/// Converts an associative array literal's pairs into a flat list of expressions.
/// String keys become `NamedArg` expressions with `ExpandedArgOrigin::StaticAssocNamed`;
/// integer-like keys become positional expressions with `ExpandedArgOrigin::StaticAssocPositional`.
/// Duplicate named keys follow PHP last-wins semantics.
fn expand_static_assoc_spread(
    pairs: &[(Expr, Expr)],
    spread_span: Span,
) -> Option<Vec<(Expr, ExpandedArgOrigin)>> {
    let mut positional_args = Vec::new();
    let mut named_args: Vec<(String, Expr)> = Vec::new();
    for (key, value) in pairs {
        match static_assoc_spread_key(key)? {
            StaticAssocSpreadKey::Positional => positional_args.push((
                Expr::new(value.kind.clone(), spread_span),
                ExpandedArgOrigin::StaticAssocPositional,
            )),
            StaticAssocSpreadKey::Named(name) => {
                if let Some((_, existing_value)) = named_args
                    .iter_mut()
                    .find(|(existing_name, _)| existing_name == &name)
                {
                    *existing_value = value.clone();
                } else {
                    named_args.push((name, value.clone()));
                }
            }
        }
    }
    let mut args = positional_args;
    args.extend(named_args.into_iter().map(|(name, value)| {
        (
            Expr::new(
                ExprKind::NamedArg {
                    name,
                    value: Box::new(value),
                },
                spread_span,
            ),
            ExpandedArgOrigin::StaticAssocNamed,
        )
    }));
    Some(args)
}

/// Classifies a single array literal key as either a positional (integer-like)
/// key or a named (string) key according to PHP array-unpack semantics.
enum StaticAssocSpreadKey {
    Positional,
    Named(String),
}

/// Returns `Some(StaticAssocSpreadKey::Positional)` for integer-like literal keys,
/// `Some(StaticAssocSpreadKey::Named(name))` for non-integer string literals,
/// or `None` if the key is not a statically resolvable literal.
fn static_assoc_spread_key(key: &Expr) -> Option<StaticAssocSpreadKey> {
    match &key.kind {
        ExprKind::IntLiteral(_) | ExprKind::BoolLiteral(_) | ExprKind::FloatLiteral(_) => {
            Some(StaticAssocSpreadKey::Positional)
        }
        ExprKind::StringLiteral(value) if crate::types::is_php_integer_array_key(value) => {
            Some(StaticAssocSpreadKey::Positional)
        }
        ExprKind::StringLiteral(value) => Some(StaticAssocSpreadKey::Named(value.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one integer expression for fixed-slot planner tests.
    fn int(value: i64) -> Expr {
        Expr::new(ExprKind::IntLiteral(value), Span::dummy())
    }

    /// Builds one explicit named argument for fixed-slot planner tests.
    fn named(name: &str, value: i64) -> Expr {
        Expr::new(
            ExprKind::NamedArg {
                name: name.to_string(),
                value: Box::new(int(value)),
            },
            Span::dummy(),
        )
    }

    /// Verifies explicit names report contiguous slots, holes, unknown names, and duplicates.
    #[test]
    fn fixed_slots_classify_explicit_named_arguments() {
        let names = ["start", "interval", "end", "options"];
        assert_eq!(
            fixed_parameter_slots(&[named("start", 1), named("interval", 2)], &names),
            FixedParameterSlots::Contiguous(2)
        );
        assert_eq!(
            fixed_parameter_slots(&[named("start", 1), named("options", 2)], &names),
            FixedParameterSlots::Missing(1)
        );
        assert_eq!(
            fixed_parameter_slots(&[named("isostr", 1)], &names),
            FixedParameterSlots::Unknown("isostr".to_string())
        );
        assert_eq!(
            fixed_parameter_slots(&[int(1), named("start", 2)], &names),
            FixedParameterSlots::Duplicate(0)
        );
    }

    /// Verifies static associative unpack keys participate in the same fixed-slot analysis.
    #[test]
    fn fixed_slots_expand_static_associative_spreads() {
        let spread = Expr::new(
            ExprKind::Spread(Box::new(Expr::new(
                ExprKind::ArrayLiteralAssoc(vec![
                    (
                        Expr::new(
                            ExprKind::StringLiteral("start".to_string()),
                            Span::dummy(),
                        ),
                        int(1),
                    ),
                    (
                        Expr::new(
                            ExprKind::StringLiteral("interval".to_string()),
                            Span::dummy(),
                        ),
                        int(2),
                    ),
                ]),
                Span::dummy(),
            ))),
            Span::dummy(),
        );
        assert_eq!(
            fixed_parameter_slots(&[spread], &["start", "interval", "end", "options"]),
            FixedParameterSlots::Contiguous(2)
        );
    }

    /// Verifies runtime spreads remain deferred to runtime overload dispatch.
    #[test]
    fn fixed_slots_defer_runtime_spreads() {
        let spread = Expr::new(
            ExprKind::Spread(Box::new(Expr::new(
                ExprKind::Variable("arguments".to_string()),
                Span::dummy(),
            ))),
            Span::dummy(),
        );
        assert_eq!(
            fixed_parameter_slots(&[spread], &["start", "interval", "end", "options"]),
            FixedParameterSlots::DynamicSpread
        );
    }
}
