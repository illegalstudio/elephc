//! Purpose:
//! Lowers source-order evaluation for named and spread arguments.
//! Works with the shared call-argument plan to preserve PHP named-argument semantics.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args::named`
//!
//! Key details:
//! - Side effects occur in source order, while final argument materialization follows parameter and ABI order.

use crate::codegen::emit::Emitter;
use crate::codegen::{context::Context, data_section::DataSection};
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::call_args;
use crate::types::FunctionSig;

use super::final_args::push_final_call_args_from_sources;
use super::prefix::emit_prefix_array_length_check;
use super::temps::{emit_source_temp_arg, push_source_temp_type};
use super::{FinalArgSource, PrefixVariadicTail, VariadicArgSource};
use super::super::{push_expr_arg, EmittedCallArgs};

pub(in crate::codegen::expr::calls::args) fn emit_source_order_named_call_args(
    args_exprs: &[Expr],
    sig: &FunctionSig,
    regular_param_count: usize,
    ref_arg_context_label: &str,
    retain_non_variable_ref_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> EmittedCallArgs {
    let plan = call_args::plan_call_args_with_regular_param_count(
        sig,
        args_exprs,
        Span::dummy(),
        regular_param_count,
        false,
        true,
    )
        .expect("codegen received invalid named call arguments after type checking");
    debug_assert!(plan.has_named_args());

    if plan.has_spread_args() {
        return emit_source_order_named_spread_call_args(
            &plan,
            sig,
            regular_param_count,
            Span::dummy(),
            emitter,
            ctx,
            data,
        );
    }

    emit_source_order_named_non_spread_call_args(
        &plan,
        sig,
        regular_param_count,
        ref_arg_context_label,
        retain_non_variable_ref_args,
        emitter,
        ctx,
        data,
    )
}

fn emit_source_order_named_non_spread_call_args(
    plan: &call_args::CallArgPlan,
    sig: &FunctionSig,
    regular_param_count: usize,
    ref_arg_context_label: &str,
    retain_non_variable_ref_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> EmittedCallArgs {
    let mut slot_sources: Vec<Option<FinalArgSource>> = vec![None; regular_param_count];
    let mut variadic_sources = Vec::new();
    let mut source_temp_types = Vec::new();
    let mut source_temp_by_index: Vec<Option<usize>> = vec![None; plan.source_args.len()];

    for source in &plan.source_values {
        match source {
            call_args::PlannedSourceValue::Regular {
                source_index,
                param_idx,
                expr,
            } => {
                let temp_idx = emit_source_temp_arg(
                    expr,
                    sig,
                    Some(*param_idx),
                    ref_arg_context_label,
                    retain_non_variable_ref_args,
                    &mut source_temp_types,
                    emitter,
                    ctx,
                    data,
                );
                source_temp_by_index[*source_index] = Some(temp_idx);
            }
            call_args::PlannedSourceValue::Variadic {
                source_index,
                key,
                expr,
            } => {
                let temp_idx = emit_source_temp_arg(
                    expr,
                    sig,
                    None,
                    ref_arg_context_label,
                    retain_non_variable_ref_args,
                    &mut source_temp_types,
                    emitter,
                    ctx,
                    data,
                );
                source_temp_by_index[*source_index] = Some(temp_idx);
                variadic_sources.push(VariadicArgSource {
                    key: key.clone(),
                    source: FinalArgSource::SourceTemp(temp_idx),
                });
            }
        }
    }

    for (idx, planned) in plan.regular_args.iter().enumerate() {
        match planned {
            call_args::PlannedRegularArg::Source { source_index, .. } => {
                let temp_idx = source_temp_by_index[*source_index]
                    .expect("planned regular source was not evaluated");
                slot_sources[idx] = Some(FinalArgSource::SourceTemp(temp_idx));
            }
            call_args::PlannedRegularArg::Default(default) => {
                slot_sources[idx] = Some(FinalArgSource::Default(default.clone()));
            }
            call_args::PlannedRegularArg::SpreadElement { .. } => {
                unreachable!("non-spread named call plan contained a spread element");
            }
        }
    }

    push_final_call_args_from_sources(
        slot_sources,
        variadic_sources,
        None,
        sig,
        regular_param_count,
        &source_temp_types,
        emitter,
        ctx,
        data,
    )
}

fn emit_source_order_named_spread_call_args(
    plan: &call_args::CallArgPlan,
    sig: &FunctionSig,
    regular_param_count: usize,
    call_span: Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> EmittedCallArgs {
    let first_named_pos = plan.first_named_pos.unwrap_or(plan.source_args.len());
    let prefix_args = &plan.source_args[..first_named_pos];
    let prefix_span = prefix_args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(Span::dummy);
    let prefix_expr = plan
        .positional_prefix_expr(call_span)
        .unwrap_or_else(|| Expr::new(ExprKind::ArrayLiteral(Vec::new()), prefix_span));
    let mut source_temp_types = Vec::new();
    emitter.comment("evaluate named-call positional prefix");
    let prefix_ty = push_expr_arg(&prefix_expr, None, emitter, ctx, data);
    let prefix_temp_idx = push_source_temp_type(&mut source_temp_types, prefix_ty);

    let mut source_temp_by_index: Vec<Option<usize>> = vec![None; plan.source_args.len()];
    let mut variadic_sources = Vec::new();
    for source in &plan.source_values {
        if source.source_index() < first_named_pos {
            continue;
        }
        let param_idx = source.param_idx();
        let temp_idx = emit_source_temp_arg(
            source.expr(),
            sig,
            param_idx,
            if param_idx.is_some() {
                "named arg"
            } else {
                "named variadic arg"
            },
            false,
            &mut source_temp_types,
            emitter,
            ctx,
            data,
        );
        source_temp_by_index[source.source_index()] = Some(temp_idx);
        if param_idx.is_none() {
            variadic_sources.push(VariadicArgSource {
                key: source.key().map(str::to_string),
                source: FinalArgSource::SourceTemp(temp_idx),
            });
        }
    }

    let fixed_max_prefix_len = plan
        .regular_args
        .iter()
        .filter_map(|planned| match planned {
            call_args::PlannedRegularArg::SpreadElement {
                prefix_element_idx,
                ..
            } => Some(prefix_element_idx + 1),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let has_later_named_regular = plan
        .source_values
        .iter()
        .any(|source| source.source_index() >= first_named_pos && source.param_idx().is_some());
    let max_prefix_len = if sig.variadic.is_some() && !has_later_named_regular {
        None
    } else {
        Some(fixed_max_prefix_len)
    };
    let min_prefix_len = plan
        .regular_args
        .iter()
        .filter_map(|planned| match planned {
            call_args::PlannedRegularArg::SpreadElement {
                prefix_element_idx,
                default,
                ..
            } if default.is_none() => Some(prefix_element_idx + 1),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    emit_prefix_array_length_check(
        prefix_temp_idx,
        &source_temp_types,
        min_prefix_len,
        max_prefix_len,
        emitter,
        ctx,
        data,
    );

    let prefix_variadic_tail = if sig.variadic.is_some() && max_prefix_len.is_none() {
        Some(PrefixVariadicTail {
            prefix_temp_idx,
            start_idx: regular_param_count,
        })
    } else {
        None
    };

    let mut slot_sources = Vec::new();
    for planned in &plan.regular_args {
        match planned {
            call_args::PlannedRegularArg::Source { source_index, .. } => {
                let temp_idx = source_temp_by_index[*source_index]
                    .expect("planned named source was not evaluated");
                slot_sources.push(Some(FinalArgSource::SourceTemp(temp_idx)));
            }
            call_args::PlannedRegularArg::SpreadElement {
                prefix_element_idx,
                default,
                guaranteed_present,
                ..
            } => {
                slot_sources.push(Some(FinalArgSource::PrefixElement {
                    prefix_temp_idx,
                    element_idx: *prefix_element_idx,
                    default: if *guaranteed_present {
                        None
                    } else {
                        default.clone()
                    },
                }));
            }
            call_args::PlannedRegularArg::Default(default) => {
                slot_sources.push(Some(FinalArgSource::Default(default.clone())));
            }
        }
    }

    push_final_call_args_from_sources(
        slot_sources,
        variadic_sources,
        prefix_variadic_tail,
        sig,
        regular_param_count,
        &source_temp_types,
        emitter,
        ctx,
        data,
    )
}
