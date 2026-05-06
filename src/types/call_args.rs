use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;

use super::FunctionSig;

#[derive(Clone)]
pub(crate) enum PrefixArg {
    Positional(Expr),
    Spread(Expr, Span),
}

pub(crate) enum NamedParamMatch {
    Regular(usize),
    Variadic,
    Unknown,
}

pub(crate) struct DuplicateNamedParam {
    pub(crate) param_idx: usize,
}

pub(crate) struct NamedParamTracker {
    assigned: Vec<bool>,
}

impl NamedParamTracker {
    pub(crate) fn new(regular_param_count: usize) -> Self {
        Self {
            assigned: vec![false; regular_param_count],
        }
    }

    pub(crate) fn assign(
        &mut self,
        sig: &FunctionSig,
        regular_param_count: usize,
        name: &str,
        allow_unknown_named_variadic: bool,
    ) -> Result<NamedParamMatch, DuplicateNamedParam> {
        match match_named_param(sig, regular_param_count, name, allow_unknown_named_variadic) {
            NamedParamMatch::Regular(param_idx) => {
                if self.assigned.get(param_idx).copied().unwrap_or(false) {
                    Err(DuplicateNamedParam { param_idx })
                } else {
                    self.assigned[param_idx] = true;
                    Ok(NamedParamMatch::Regular(param_idx))
                }
            }
            other => Ok(other),
        }
    }
}

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

    let mut args = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        match static_assoc_spread_key(key)? {
            StaticAssocSpreadKey::Positional => args.push(value.clone()),
            StaticAssocSpreadKey::Named(name) => args.push(Expr::new(
                ExprKind::NamedArg {
                    name,
                    value: Box::new(value.clone()),
                },
                spread_span,
            )),
        }
    }
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

pub(crate) fn regular_param_count(sig: &FunctionSig) -> usize {
    if sig.variadic.is_some() {
        sig.params.len().saturating_sub(1)
    } else {
        sig.params.len()
    }
}

pub(crate) fn named_param_index(
    sig: &FunctionSig,
    regular_param_count: usize,
    name: &str,
) -> Option<usize> {
    sig.params
        .iter()
        .take(regular_param_count)
        .position(|(param_name, _)| param_name == name)
}

pub(crate) fn match_named_param(
    sig: &FunctionSig,
    regular_param_count: usize,
    name: &str,
    allow_unknown_named_variadic: bool,
) -> NamedParamMatch {
    if let Some(param_idx) = named_param_index(sig, regular_param_count, name) {
        NamedParamMatch::Regular(param_idx)
    } else if allow_unknown_named_variadic && sig.variadic.is_some() {
        NamedParamMatch::Variadic
    } else {
        NamedParamMatch::Unknown
    }
}
