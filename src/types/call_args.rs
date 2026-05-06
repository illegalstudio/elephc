use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::span::Span;

use super::FunctionSig;

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

#[derive(Clone)]
pub(crate) struct CallArgPlan {
    pub(crate) source_args: Vec<Expr>,
    pub(crate) regular_args: Vec<PlannedRegularArg>,
    pub(crate) variadic_args: Vec<PlannedVariadicArg>,
    pub(crate) source_values: Vec<PlannedSourceValue>,
    pub(crate) spread_bounds_checks: Vec<SpreadBoundsCheck>,
    pub(crate) first_named_pos: Option<usize>,
    passthrough_args: Option<Vec<Expr>>,
}

#[derive(Clone)]
pub(crate) enum PlannedRegularArg {
    Source {
        source_index: usize,
        expr: Expr,
    },
    SpreadElement {
        spread_expr: Expr,
        spread_span: Span,
        element_idx: usize,
        prefix_element_idx: usize,
        default: Option<Expr>,
    },
    Default(Expr),
}

#[derive(Clone)]
pub(crate) struct PlannedVariadicArg {
    pub(crate) key: Option<String>,
    pub(crate) expr: Expr,
}

#[derive(Clone)]
pub(crate) enum PlannedSourceValue {
    Regular {
        source_index: usize,
        param_idx: usize,
        expr: Expr,
    },
    Variadic {
        source_index: usize,
        key: Option<String>,
        expr: Expr,
    },
}

#[derive(Clone)]
pub(crate) struct SpreadBoundsCheck {
    pub(crate) spread_expr: Expr,
    pub(crate) min_len: usize,
    pub(crate) max_len: Option<usize>,
}

#[derive(Debug)]
pub(crate) enum CallArgPlanError {
    UnknownNamed {
        span: Span,
        name: String,
    },
    Duplicate {
        span: Span,
        param_idx: usize,
        name: String,
    },
    PositionalAfterNamed {
        span: Span,
    },
    PositionalAfterSpread {
        span: Span,
    },
    SpreadAfterNamed {
        span: Span,
    },
    MissingRequired {
        span: Span,
        param_idx: usize,
    },
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

impl CallArgPlan {
    pub(crate) fn has_named_args(&self) -> bool {
        self.first_named_pos.is_some()
    }

    pub(crate) fn has_spread_args(&self) -> bool {
        self.source_args
            .iter()
            .any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
    }

    pub(crate) fn normalized_args(&self) -> Vec<Expr> {
        if let Some(args) = &self.passthrough_args {
            return args.clone();
        }

        let mut args = Vec::new();
        for arg in &self.regular_args {
            args.push(match arg {
                PlannedRegularArg::Source { expr, .. } => expr.clone(),
                PlannedRegularArg::SpreadElement {
                    spread_expr,
                    spread_span,
                    element_idx,
                    default,
                    ..
                } => {
                    if let Some(default) = default {
                        spread_element_or_default_expr(
                            spread_expr,
                            *element_idx,
                            default.clone(),
                            *spread_span,
                        )
                    } else {
                        spread_element_expr(spread_expr, *element_idx, *spread_span)
                    }
                }
                PlannedRegularArg::Default(default) => default.clone(),
            });
        }

        for arg in &self.variadic_args {
            if let Some(key) = &arg.key {
                args.push(Expr::new(
                    ExprKind::NamedArg {
                        name: key.clone(),
                        value: Box::new(arg.expr.clone()),
                    },
                    arg.expr.span,
                ));
            } else {
                args.push(arg.expr.clone());
            }
        }
        args
    }

    pub(crate) fn positional_prefix_expr(&self, call_span: Span) -> Option<Expr> {
        let first_named_pos = self.first_named_pos?;
        let prefix_args = self.source_args[..first_named_pos].to_vec();
        let prefix_span = prefix_args
            .first()
            .map(|arg| arg.span)
            .unwrap_or(call_span);
        if let [arg] = prefix_args.as_slice() {
            if let ExprKind::Spread(inner) = &arg.kind {
                return Some((**inner).clone());
            }
        }
        Some(Expr::new(ExprKind::ArrayLiteral(prefix_args), prefix_span))
    }
}

impl PlannedSourceValue {
    pub(crate) fn source_index(&self) -> usize {
        match self {
            PlannedSourceValue::Regular { source_index, .. }
            | PlannedSourceValue::Variadic { source_index, .. } => *source_index,
        }
    }

    pub(crate) fn param_idx(&self) -> Option<usize> {
        match self {
            PlannedSourceValue::Regular { param_idx, .. } => Some(*param_idx),
            PlannedSourceValue::Variadic { .. } => None,
        }
    }

    pub(crate) fn key(&self) -> Option<&str> {
        match self {
            PlannedSourceValue::Regular { .. } => None,
            PlannedSourceValue::Variadic { key, .. } => key.as_deref(),
        }
    }

    pub(crate) fn expr(&self) -> &Expr {
        match self {
            PlannedSourceValue::Regular { expr, .. }
            | PlannedSourceValue::Variadic { expr, .. } => expr,
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

pub(crate) fn plan_call_args(
    sig: &FunctionSig,
    args: &[Expr],
    call_span: Span,
    trim_trailing_defaults: bool,
    allow_unknown_named_variadic: bool,
) -> Result<CallArgPlan, CallArgPlanError> {
    plan_call_args_with_regular_param_count(
        sig,
        args,
        call_span,
        regular_param_count(sig),
        trim_trailing_defaults,
        allow_unknown_named_variadic,
    )
}

pub(crate) fn plan_call_args_with_regular_param_count(
    sig: &FunctionSig,
    args: &[Expr],
    call_span: Span,
    regular_param_count: usize,
    trim_trailing_defaults: bool,
    allow_unknown_named_variadic: bool,
) -> Result<CallArgPlan, CallArgPlanError> {
    let source_args = expand_static_assoc_spread_args(args);
    let first_named_pos = source_args
        .iter()
        .position(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }));

    if first_named_pos.is_none() {
        validate_positional_spread_order(&source_args)?;
        return Ok(CallArgPlan {
            passthrough_args: Some(source_args.clone()),
            source_args,
            regular_args: Vec::new(),
            variadic_args: Vec::new(),
            source_values: Vec::new(),
            spread_bounds_checks: Vec::new(),
            first_named_pos,
        });
    }

    plan_named_call_args(
        sig,
        source_args,
        call_span,
        regular_param_count,
        trim_trailing_defaults,
        allow_unknown_named_variadic,
        first_named_pos,
    )
}

fn validate_positional_spread_order(args: &[Expr]) -> Result<(), CallArgPlanError> {
    let mut seen_spread = false;
    for arg in args {
        if matches!(arg.kind, ExprKind::Spread(_)) {
            seen_spread = true;
        } else if seen_spread {
            return Err(CallArgPlanError::PositionalAfterSpread { span: arg.span });
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn plan_named_call_args(
    sig: &FunctionSig,
    source_args: Vec<Expr>,
    call_span: Span,
    regular_param_count: usize,
    trim_trailing_defaults: bool,
    allow_unknown_named_variadic: bool,
    first_named_pos: Option<usize>,
) -> Result<CallArgPlan, CallArgPlanError> {
    let mut named_values: Vec<Option<NamedValue>> = vec![None; regular_param_count];
    let mut named_tracker = NamedParamTracker::new(regular_param_count);
    let mut prefix_args = Vec::new();
    let mut variadic_args = Vec::new();
    let mut source_values: Vec<Option<PlannedSourceValue>> = vec![None; source_args.len()];
    let mut seen_named = false;
    let mut seen_spread = false;

    for (source_index, arg) in source_args.iter().enumerate() {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                seen_named = true;
                match named_tracker.assign(
                    sig,
                    regular_param_count,
                    name,
                    allow_unknown_named_variadic,
                ) {
                    Ok(NamedParamMatch::Regular(param_idx)) => {
                        let expr = (**value).clone();
                        named_values[param_idx] = Some(NamedValue {
                            source_index,
                            expr: expr.clone(),
                            span: arg.span,
                            name: name.clone(),
                        });
                        source_values[source_index] = Some(PlannedSourceValue::Regular {
                            source_index,
                            param_idx,
                            expr,
                        });
                    }
                    Ok(NamedParamMatch::Variadic) => {
                        let expr = (**value).clone();
                        variadic_args.push(PlannedVariadicArg {
                            key: Some(name.clone()),
                            expr: expr.clone(),
                        });
                        source_values[source_index] = Some(PlannedSourceValue::Variadic {
                            source_index,
                            key: Some(name.clone()),
                            expr,
                        });
                    }
                    Ok(NamedParamMatch::Unknown) => {
                        return Err(CallArgPlanError::UnknownNamed {
                            span: arg.span,
                            name: name.clone(),
                        });
                    }
                    Err(duplicate) => {
                        return Err(CallArgPlanError::Duplicate {
                            span: arg.span,
                            param_idx: duplicate.param_idx,
                            name: name.clone(),
                        });
                    }
                }
            }
            ExprKind::Spread(inner) => {
                if seen_named {
                    return Err(CallArgPlanError::SpreadAfterNamed { span: arg.span });
                }
                seen_spread = true;
                prefix_args.push(PrefixSourceArg::Spread {
                    expr: (**inner).clone(),
                    span: arg.span,
                });
            }
            _ => {
                if seen_named {
                    return Err(CallArgPlanError::PositionalAfterNamed { span: arg.span });
                }
                if seen_spread {
                    return Err(CallArgPlanError::PositionalAfterSpread { span: arg.span });
                }
                prefix_args.push(PrefixSourceArg::Positional {
                    source_index,
                    expr: arg.clone(),
                });
            }
        }
    }

    let mut resolved: Vec<Option<PlannedRegularArg>> = vec![None; regular_param_count];
    let mut positional_idx = 0usize;
    let mut spread_bounds_checks = Vec::new();

    for prefix_arg in prefix_args {
        match prefix_arg {
            PrefixSourceArg::Positional { source_index, expr } => {
                while positional_idx < regular_param_count {
                    let Some(named_value) = &named_values[positional_idx] else {
                        break;
                    };
                    if same_span(expr.span, named_value.span) {
                        positional_idx += 1;
                    } else {
                        return Err(CallArgPlanError::Duplicate {
                            span: named_value.span,
                            param_idx: positional_idx,
                            name: named_value.name.clone(),
                        });
                    }
                }
                if positional_idx < regular_param_count {
                    resolved[positional_idx] = Some(PlannedRegularArg::Source {
                        source_index,
                        expr: expr.clone(),
                    });
                    source_values[source_index] = Some(PlannedSourceValue::Regular {
                        source_index,
                        param_idx: positional_idx,
                        expr,
                    });
                } else {
                    variadic_args.push(PlannedVariadicArg {
                        key: None,
                        expr: expr.clone(),
                    });
                    source_values[source_index] = Some(PlannedSourceValue::Variadic {
                        source_index,
                        key: None,
                        expr,
                    });
                }
                positional_idx += 1;
            }
            PrefixSourceArg::Spread {
                expr,
                span,
            } => {
                let next_named_idx = (positional_idx..regular_param_count)
                    .find(|idx| named_values[*idx].is_some())
                    .unwrap_or(regular_param_count);
                let max_len = next_named_idx.saturating_sub(positional_idx);
                let has_regular_named_bound = next_named_idx < regular_param_count;
                let upper_bound = if sig.variadic.is_some() && !has_regular_named_bound {
                    None
                } else {
                    Some(max_len)
                };
                let min_len = (positional_idx..next_named_idx)
                    .rfind(|idx| sig.defaults.get(*idx).and_then(|default| default.as_ref()).is_none())
                    .map(|idx| idx - positional_idx + 1)
                    .unwrap_or(0);
                spread_bounds_checks.push(SpreadBoundsCheck {
                    spread_expr: expr.clone(),
                    min_len,
                    max_len: upper_bound,
                });
                for element_idx in 0..max_len {
                    let prefix_element_idx = positional_idx;
                    let default = sig
                        .defaults
                        .get(positional_idx)
                        .and_then(|default| default.clone());
                    resolved[positional_idx] = Some(PlannedRegularArg::SpreadElement {
                        spread_expr: expr.clone(),
                        spread_span: span,
                        element_idx,
                        prefix_element_idx,
                        default,
                    });
                    positional_idx += 1;
                }
            }
        }
    }

    for (idx, named_value) in named_values.into_iter().enumerate() {
        if let Some(named_value) = named_value {
            if resolved[idx].is_some() {
                return Err(CallArgPlanError::Duplicate {
                    span: named_value.span,
                    param_idx: idx,
                    name: named_value.name,
                });
            }
            resolved[idx] = Some(PlannedRegularArg::Source {
                source_index: named_value.source_index,
                expr: named_value.expr,
            });
        }
    }

    let output_len = if trim_trailing_defaults {
        resolved
            .iter()
            .rposition(|slot| slot.is_some())
            .map(|idx| idx + 1)
            .unwrap_or(0)
    } else {
        regular_param_count
    };
    let mut regular_args = Vec::new();
    for (idx, slot) in resolved.into_iter().take(output_len).enumerate() {
        if let Some(arg) = slot {
            regular_args.push(arg);
        } else if let Some(Some(default_expr)) = sig.defaults.get(idx) {
            regular_args.push(PlannedRegularArg::Default(default_expr.clone()));
        } else {
            return Err(CallArgPlanError::MissingRequired {
                span: call_span,
                param_idx: idx,
            });
        }
    }

    Ok(CallArgPlan {
        source_args,
        regular_args,
        variadic_args,
        source_values: source_values.into_iter().flatten().collect(),
        spread_bounds_checks,
        first_named_pos,
        passthrough_args: None,
    })
}

#[derive(Clone)]
struct NamedValue {
    source_index: usize,
    expr: Expr,
    span: Span,
    name: String,
}

enum PrefixSourceArg {
    Positional {
        source_index: usize,
        expr: Expr,
    },
    Spread {
        expr: Expr,
        span: Span,
    },
}

fn same_span(left: Span, right: Span) -> bool {
    left.line == right.line && left.col == right.col
}

fn spread_element_expr(spread_expr: &Expr, element_idx: usize, span: Span) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(spread_expr.clone()),
            index: Box::new(Expr::new(ExprKind::IntLiteral(element_idx as i64), span)),
        },
        span,
    )
}

fn spread_element_or_default_expr(
    spread_expr: &Expr,
    element_idx: usize,
    default_expr: Expr,
    span: Span,
) -> Expr {
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(spread_len_expr(spread_expr, span)),
                    op: BinOp::Gt,
                    right: Box::new(Expr::new(ExprKind::IntLiteral(element_idx as i64), span)),
                },
                span,
            )),
            then_expr: Box::new(spread_element_expr(spread_expr, element_idx, span)),
            else_expr: Box::new(default_expr),
        },
        span,
    )
}

fn spread_len_expr(spread_expr: &Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("count"),
            args: vec![spread_expr.clone()],
        },
        span,
    )
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
