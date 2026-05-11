//! Purpose:
//! Plans PHP call arguments from source order into regular and variadic parameter slots.
//! Validates named/positional ordering, duplicate parameters, spreads, defaults, and unknown variadic names.
//!
//! Called from:
//! - `crate::types::call_args::plan_call_args()`
//! - `crate::types::call_args::plan_call_args_with_regular_param_count()`
//!
//! Key details:
//! - Source evaluation order is preserved separately from ABI/materialization order for codegen.

use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::FunctionSig;

use super::matching::{regular_param_count, NamedParamMatch, NamedParamTracker};
use super::plan::{
    CallArgPlan, CallArgPlanError, PlannedRegularArg, PlannedSourceValue, PlannedVariadicArg,
    SpreadBoundsCheck,
};
use super::static_spread::expand_static_assoc_spread_args;

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
                    let guaranteed_present = element_idx < min_len;
                    resolved[positional_idx] = Some(PlannedRegularArg::SpreadElement {
                        spread_expr: expr.clone(),
                        spread_span: span,
                        element_idx,
                        prefix_element_idx,
                        default,
                        guaranteed_present,
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
