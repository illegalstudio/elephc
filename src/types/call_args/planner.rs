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
use super::static_spread::{
    expand_static_assoc_spread_args_with_origins, ExpandedArgOrigin,
};

/// Validates and normalizes call-site arguments against `sig`, inferring the
/// caller-visible regular parameter count from the signature.
///
/// - `trim_trailing_defaults`: when `true`, elide trailing default-only slots from the plan.
/// - `allow_unknown_named_variadic`: when `true`, unknown named args are allowed and routed
///   to the variadic parameter if the signature is variadic.
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

/// Validates and normalizes call-site arguments against `sig` using an explicitly
/// supplied `regular_param_count` rather than inferring it from the signature.
/// Use this when the caller knows the visible parameter count (e.g., internal
/// signatures with hidden implementation parameters).
pub(crate) fn plan_call_args_with_regular_param_count(
    sig: &FunctionSig,
    args: &[Expr],
    call_span: Span,
    regular_param_count: usize,
    trim_trailing_defaults: bool,
    allow_unknown_named_variadic: bool,
) -> Result<CallArgPlan, CallArgPlanError> {
    let expanded = expand_static_assoc_spread_args_with_origins(args);
    let assoc_spread_sources = vec![false; expanded.args.len()];
    let (source_args, source_origins, assoc_spread_sources) =
        expand_static_tail_spreads_after_unpack_named(
            expanded.args,
            expanded.origins,
            assoc_spread_sources,
        );
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
            prefix_has_dynamic_named_spread: false,
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
        &source_origins,
        &assoc_spread_sources,
    )
}

/// Like `plan_call_args_with_regular_param_count` but accepts a caller-supplied
/// `assoc_spread_sources` vector that marks which arguments come from associative
/// spread expansions. Used by codegen when propagating spread-source information
/// through multiple call layers.
pub(crate) fn plan_call_args_with_regular_param_count_and_assoc_spreads(
    sig: &FunctionSig,
    args: &[Expr],
    call_span: Span,
    regular_param_count: usize,
    trim_trailing_defaults: bool,
    allow_unknown_named_variadic: bool,
    assoc_spread_sources: &[bool],
) -> Result<CallArgPlan, CallArgPlanError> {
    let expanded = expand_static_assoc_spread_args_with_origins(args);
    let expanded_assoc_spread_sources = (0..expanded.args.len())
        .map(|idx| assoc_spread_sources.get(idx).copied().unwrap_or(false))
        .collect();
    let (source_args, source_origins, assoc_spread_sources) =
        expand_static_tail_spreads_after_unpack_named(
            expanded.args,
            expanded.origins,
            expanded_assoc_spread_sources,
        );
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
            prefix_has_dynamic_named_spread: false,
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
        &source_origins,
        &assoc_spread_sources,
    )
}

/// Returns `Ok` if no positional argument appears after a spread expression,
/// otherwise returns `PositionalAfterSpread` for the offending argument.
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

/// Core named-argument planning: resolves named params, spreads, positional prefixes,
/// and defaults into a `CallArgPlan`. Handles duplicate detection, ordering constraints,
/// spread bounds, and dynamic named prefixes.
#[allow(clippy::too_many_arguments)]
fn plan_named_call_args(
    sig: &FunctionSig,
    source_args: Vec<Expr>,
    call_span: Span,
    regular_param_count: usize,
    trim_trailing_defaults: bool,
    allow_unknown_named_variadic: bool,
    first_named_pos: Option<usize>,
    source_origins: &[ExpandedArgOrigin],
    assoc_spread_sources: &[bool],
) -> Result<CallArgPlan, CallArgPlanError> {
    let mut named_values: Vec<Option<NamedValue>> = vec![None; regular_param_count];
    let mut named_tracker = NamedParamTracker::new(regular_param_count);
    let mut prefix_args = Vec::new();
    let mut variadic_args = Vec::new();
    let mut source_values: Vec<Option<PlannedSourceValue>> = vec![None; source_args.len()];
    let mut seen_named = false;
    let mut seen_spread = false;

    for (source_index, arg) in source_args.iter().enumerate() {
        let origin = source_origins
            .get(source_index)
            .copied()
            .unwrap_or(ExpandedArgOrigin::Source);
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                seen_named = true;
                let is_static_assoc_named = origin == ExpandedArgOrigin::StaticAssocNamed;
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
                        if is_static_assoc_named {
                            prefix_args.push(PrefixSourceArg::StaticNamedCursor { param_idx });
                        }
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
                    is_assoc_named_provider: assoc_spread_sources
                        .get(source_index)
                        .copied()
                        .unwrap_or(false),
                });
            }
            _ => {
                let is_static_tail_positional =
                    origin == ExpandedArgOrigin::StaticTailPositional;
                if seen_named && !is_static_tail_positional {
                    return Err(CallArgPlanError::PositionalAfterNamed { span: arg.span });
                }
                if seen_spread && !is_static_tail_positional {
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
    let dynamic_prefix_expr = dynamic_named_prefix_expr(&prefix_args, call_span);
    let prefix_has_dynamic_named_spread = dynamic_prefix_expr.is_some();

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
                is_assoc_named_provider,
                ..
            } => {
                if is_assoc_named_provider {
                    continue;
                }
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
                        param_name: sig.params.get(positional_idx).map(|(name, _)| name.clone()),
                        prefer_named_key: false,
                        default,
                        guaranteed_present,
                    });
                    positional_idx += 1;
                }
            }
            PrefixSourceArg::StaticNamedCursor { param_idx } => {
                positional_idx = positional_idx.max(param_idx + 1);
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

    if let Some(dynamic_prefix_expr) = dynamic_prefix_expr {
        for (idx, slot) in resolved.iter_mut().enumerate() {
            if slot.is_some() {
                continue;
            }
            let default = sig.defaults.get(idx).and_then(|default| default.clone());
            *slot = Some(PlannedRegularArg::SpreadElement {
                spread_expr: dynamic_prefix_expr.clone(),
                spread_span: dynamic_prefix_expr.span,
                element_idx: idx,
                prefix_element_idx: idx,
                param_name: sig.params.get(idx).map(|(name, _)| name.clone()),
                prefer_named_key: true,
                default,
                guaranteed_present: false,
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
        prefix_has_dynamic_named_spread,
        passthrough_args: None,
    })
}

/// Temporary tracker for a named argument that was resolved to a regular parameter slot.
#[derive(Clone)]
struct NamedValue {
    source_index: usize,
    expr: Expr,
    span: Span,
    name: String,
}

/// A source argument being accumulated in the positional prefix before named args begin.
/// Used to distinguish positional expressions, spread expressions, and static-named-cursor
/// markers that track progress through a `...unpack_named` array.
enum PrefixSourceArg {
    Positional {
        source_index: usize,
        expr: Expr,
    },
    Spread {
        expr: Expr,
        span: Span,
        is_assoc_named_provider: bool,
    },
    StaticNamedCursor {
        param_idx: usize,
    },
}

/// Returns the spread expression from the first `PrefixSourceArg::Spread` that
/// is marked `is_assoc_named_provider`, or `None` if no such spread is present.
/// This represents the dynamic named-prefix array that must fill remaining parameter
/// slots when static analysis cannot determine which keys are present.
fn dynamic_named_prefix_expr(prefix_args: &[PrefixSourceArg], _call_span: Span) -> Option<Expr> {
    if let Some(PrefixSourceArg::Spread { expr, .. }) = prefix_args
        .iter()
        .find(|arg| matches!(arg, PrefixSourceArg::Spread { is_assoc_named_provider: true, .. }))
    {
        return Some(expr.clone());
    }
    None
}

/// Returns `true` if the two spans refer to the same line and column.
fn same_span(left: Span, right: Span) -> bool {
    left.line == right.line && left.col == right.col
}

/// After expanding static associative spreads, checks whether any `Spread` argument
/// appears after a static `...unpack_named` with no direct named args between them.
/// If so, the spread's elements become additional positional arguments (tail elements)
/// rather than feeding into named-parameter slots.
fn expand_static_tail_spreads_after_unpack_named(
    args: Vec<Expr>,
    origins: Vec<ExpandedArgOrigin>,
    assoc_spread_sources: Vec<bool>,
) -> (Vec<Expr>, Vec<ExpandedArgOrigin>, Vec<bool>) {
    let mut expanded_args = Vec::with_capacity(args.len());
    let mut expanded_origins = Vec::with_capacity(origins.len());
    let mut expanded_assoc_sources = Vec::with_capacity(assoc_spread_sources.len());
    let mut seen_static_unpack_named = false;
    let mut seen_direct_named = false;

    for (idx, arg) in args.into_iter().enumerate() {
        let origin = origins.get(idx).copied().unwrap_or(ExpandedArgOrigin::Source);
        let assoc_source = assoc_spread_sources.get(idx).copied().unwrap_or(false);

        match &arg.kind {
            ExprKind::NamedArg { .. } => {
                if origin == ExpandedArgOrigin::StaticAssocNamed {
                    seen_static_unpack_named = true;
                } else {
                    seen_direct_named = true;
                }
            }
            ExprKind::Spread(inner) if seen_static_unpack_named && !seen_direct_named => {
                if let Some(elements) = static_positional_tail_elements(inner) {
                    for element in elements {
                        expanded_args.push(element);
                        expanded_origins.push(ExpandedArgOrigin::StaticTailPositional);
                        expanded_assoc_sources.push(false);
                    }
                    continue;
                }
            }
            _ => {}
        }

        expanded_args.push(arg);
        expanded_origins.push(origin);
        expanded_assoc_sources.push(assoc_source);
    }

    (expanded_args, expanded_origins, expanded_assoc_sources)
}

/// If `expr` is a plain `ArrayLiteral` with no nested spreads, returns its elements
/// as positional tail arguments that can be inlined after a `...unpack_named` spread.
fn static_positional_tail_elements(expr: &Expr) -> Option<Vec<Expr>> {
    let ExprKind::ArrayLiteral(elements) = &expr.kind else {
        return None;
    };
    if elements
        .iter()
        .any(|element| matches!(element.kind, ExprKind::Spread(_)))
    {
        return None;
    }
    Some(elements.clone())
}
