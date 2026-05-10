//! Purpose:
//! Expands static associative spread arguments into named argument forms.
//! Recognizes PHP array-unpack keys that can be resolved at compile time before planning.
//!
//! Called from:
//! - `crate::types::call_args::planner`
//!
//! Key details:
//! - String keys become named arguments, numeric keys remain positional, and duplicate static keys follow PHP last-wins behavior.

use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;

pub(crate) fn has_named_args(args: &[Expr]) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::NamedArg { .. } => true,
        ExprKind::Spread(inner) => static_assoc_spread_has_named_args(inner),
        _ => false,
    })
}

pub(crate) fn expand_static_assoc_spread_args(args: &[Expr]) -> Vec<Expr> {
    let mut expanded = Vec::with_capacity(args.len());
    let mut changed = false;

    for arg in args {
        if let ExprKind::Spread(inner) = &arg.kind {
            if let Some(mut spread_args) = expand_static_assoc_spread(inner, arg.span) {
                changed = true;
                expanded.append(&mut spread_args);
                continue;
            }
        }
        expanded.push(arg.clone());
    }

    if changed { expanded } else { args.to_vec() }
}

fn static_assoc_spread_has_named_args(expr: &Expr) -> bool {
    let ExprKind::ArrayLiteralAssoc(pairs) = &expr.kind else {
        return false;
    };
    pairs.iter().any(|(key, _)| {
        matches!(static_assoc_spread_key(key), Some(StaticAssocSpreadKey::Named(_)))
    })
}

fn expand_static_assoc_spread(expr: &Expr, spread_span: Span) -> Option<Vec<Expr>> {
    let ExprKind::ArrayLiteralAssoc(pairs) = &expr.kind else {
        return None;
    };

    let mut positional_args = Vec::new();
    let mut named_args: Vec<(String, Expr)> = Vec::new();
    for (key, value) in pairs {
        match static_assoc_spread_key(key)? {
            StaticAssocSpreadKey::Positional => positional_args.push(Expr::new(
                value.kind.clone(),
                spread_span,
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
        Expr::new(
            ExprKind::NamedArg {
                name,
                value: Box::new(value),
            },
            spread_span,
        )
    }));
    Some(args)
}

enum StaticAssocSpreadKey {
    Positional,
    Named(String),
}

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
