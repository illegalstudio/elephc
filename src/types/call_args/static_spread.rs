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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExpandedArgOrigin {
    Source,
    StaticAssocPositional,
    StaticAssocNamed,
    StaticTailPositional,
}

pub(crate) struct ExpandedStaticSpreadArgs {
    pub(crate) args: Vec<Expr>,
    pub(crate) origins: Vec<ExpandedArgOrigin>,
}

pub(crate) fn has_named_args(args: &[Expr]) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::NamedArg { .. } => true,
        ExprKind::Spread(inner) => static_assoc_spread_has_named_args(inner),
        _ => false,
    })
}

pub(crate) fn expand_static_assoc_spread_args(args: &[Expr]) -> Vec<Expr> {
    expand_static_assoc_spread_args_with_origins(args).args
}

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

fn static_assoc_spread_has_named_args(expr: &Expr) -> bool {
    let ExprKind::ArrayLiteralAssoc(pairs) = &expr.kind else {
        return false;
    };
    pairs.iter().any(|(key, _)| {
        matches!(static_assoc_spread_key(key), Some(StaticAssocSpreadKey::Named(_)))
    })
}

fn expand_static_spread(
    expr: &Expr,
    spread_span: Span,
) -> Option<Vec<(Expr, ExpandedArgOrigin)>> {
    match &expr.kind {
        ExprKind::ArrayLiteralAssoc(pairs) => expand_static_assoc_spread(pairs, spread_span),
        _ => None,
    }
}

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
