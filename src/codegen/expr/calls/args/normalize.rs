//! Purpose:
//! Lowers preevaluation and normalization before ABI materialization.
//! Converts evaluated PHP argument expressions into temporary values ready for ABI assignment.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args`
//!
//! Key details:
//! - Argument checks must happen at PHP-observable points without skipping later side effects.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::call_args;
use crate::types::FunctionSig;

use super::{NormalizedCallArgs, PreparedCallArgs};

pub(crate) fn has_named_args(args: &[Expr]) -> bool {
    call_args::has_named_args(args)
}

pub(crate) fn regular_param_count(sig: Option<&FunctionSig>, fallback_arg_count: usize) -> usize {
    sig.map(call_args::regular_param_count)
    .unwrap_or(fallback_arg_count)
}

pub(crate) fn named_call_arg_temp_name(call_span: Span, idx: usize) -> String {
    format!(
        "__elephc_named_arg_{}_{}_{}",
        call_span.line, call_span.col, idx
    )
}

pub(crate) fn named_call_prefix_temp_name(call_span: Span) -> String {
    format!("__elephc_named_prefix_{}_{}", call_span.line, call_span.col)
}

pub(crate) fn normalize_named_call_args_with_checks(
    sig: &FunctionSig,
    args: &[Expr],
    regular_param_count: usize,
) -> NormalizedCallArgs {
    normalize_call_args(sig, args, regular_param_count, false, true)
}

pub(crate) fn normalize_builtin_call_args_with_checks(
    sig: &FunctionSig,
    args: &[Expr],
) -> NormalizedCallArgs {
    normalize_call_args(
        sig,
        args,
        regular_param_count(Some(sig), args.len()),
        true,
        false,
    )
}

pub(crate) fn preevaluate_named_call_args_to_temps(
    sig: &FunctionSig,
    args: &[Expr],
    call_span: Span,
    regular_param_count: usize,
    trim_trailing_defaults: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> NormalizedCallArgs {
    let expanded_args = call_args::expand_static_assoc_spread_args(args);
    let args = expanded_args.as_slice();

    if !has_named_args(args) {
        return normalize_call_args(
            sig,
            args,
            regular_param_count,
            trim_trailing_defaults,
            false,
        );
    }

    let rewritten = if args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
        preevaluate_named_spread_args_to_temps(sig, args, call_span, regular_param_count, emitter, ctx, data)
    } else {
        preevaluate_named_non_spread_args_to_temps(
            sig,
            args,
            call_span,
            regular_param_count,
            emitter,
            ctx,
            data,
        )
    };
    normalize_call_args(
        sig,
        &rewritten,
        regular_param_count,
        trim_trailing_defaults,
        false,
    )
}

fn preevaluate_named_spread_args_to_temps(
    sig: &FunctionSig,
    args: &[Expr],
    call_span: Span,
    regular_param_count: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<Expr> {
    let first_named_pos = args
        .iter()
        .position(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
        .unwrap_or(args.len());
    let prefix_args = args[..first_named_pos].to_vec();
    let prefix_span = prefix_args
        .first()
        .map(|arg| arg.span)
        .unwrap_or(call_span);
    let prefix_name = named_call_prefix_temp_name(call_span);
    let prefix_expr = single_spread_inner(&prefix_args)
        .unwrap_or_else(|| Expr::new(ExprKind::ArrayLiteral(prefix_args), prefix_span));
    crate::codegen::stmt::emit_assign_stmt(&prefix_name, &prefix_expr, emitter, ctx, data);

    let mut rewritten = vec![Expr::new(
        ExprKind::Spread(Box::new(Expr::new(
            ExprKind::Variable(prefix_name),
            prefix_span,
        ))),
        prefix_span,
    )];

    for (idx, arg) in args.iter().enumerate().skip(first_named_pos) {
        if let ExprKind::NamedArg { name, value } = &arg.kind {
            let rewritten_value =
                preevaluate_named_value_if_needed(sig, regular_param_count, call_span, idx, name, value, emitter, ctx, data);
            rewritten.push(Expr::new(
                ExprKind::NamedArg {
                    name: name.clone(),
                    value: Box::new(rewritten_value),
                },
                arg.span,
            ));
        }
    }

    rewritten
}

fn preevaluate_named_non_spread_args_to_temps(
    sig: &FunctionSig,
    args: &[Expr],
    call_span: Span,
    regular_param_count: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<Expr> {
    let mut rewritten = Vec::new();
    let mut positional_idx = 0usize;

    for (idx, arg) in args.iter().enumerate() {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                let rewritten_value =
                    preevaluate_named_value_if_needed(sig, regular_param_count, call_span, idx, name, value, emitter, ctx, data);
                rewritten.push(Expr::new(
                    ExprKind::NamedArg {
                        name: name.clone(),
                        value: Box::new(rewritten_value),
                    },
                    arg.span,
                ));
            }
            _ => {
                let is_ref = sig
                    .ref_params
                    .get(positional_idx)
                    .copied()
                    .unwrap_or(false);
                if is_ref || is_side_effect_free_literal(arg) {
                    rewritten.push(arg.clone());
                } else {
                    let temp_name = named_call_arg_temp_name(call_span, idx);
                    crate::codegen::stmt::emit_assign_stmt(&temp_name, arg, emitter, ctx, data);
                    rewritten.push(Expr::new(ExprKind::Variable(temp_name), arg.span));
                }
                positional_idx += 1;
            }
        }
    }

    rewritten
}

#[allow(clippy::too_many_arguments)]
fn preevaluate_named_value_if_needed(
    sig: &FunctionSig,
    regular_param_count: usize,
    call_span: Span,
    arg_idx: usize,
    name: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Expr {
    let is_ref = call_args::named_param_index(sig, regular_param_count, name)
        .and_then(|param_idx| sig.ref_params.get(param_idx))
        .copied()
        .unwrap_or(false);
    if is_ref || is_side_effect_free_literal(value) {
        return value.clone();
    }

    let temp_name = named_call_arg_temp_name(call_span, arg_idx);
    crate::codegen::stmt::emit_assign_stmt(&temp_name, value, emitter, ctx, data);
    Expr::new(ExprKind::Variable(temp_name), value.span)
}

fn single_spread_inner(prefix_args: &[Expr]) -> Option<Expr> {
    if let [arg] = prefix_args {
        if let ExprKind::Spread(inner) = &arg.kind {
            return Some((**inner).clone());
        }
    }
    None
}

fn is_side_effect_free_literal(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
    )
}

fn normalize_call_args(
    sig: &FunctionSig,
    args: &[Expr],
    regular_param_count: usize,
    trim_trailing_defaults: bool,
    allow_unknown_named_variadic: bool,
) -> NormalizedCallArgs {
    let plan = call_args::plan_call_args_with_regular_param_count(
        sig,
        args,
        Span::dummy(),
        regular_param_count,
        trim_trailing_defaults,
        allow_unknown_named_variadic,
    )
    .expect("codegen received invalid call arguments after type checking");
    NormalizedCallArgs {
        args: plan.normalized_args(),
        spread_length_checks: plan.spread_bounds_checks,
    }
}

pub(crate) fn prepare_call_args(
    sig: Option<&FunctionSig>,
    args_exprs: &[Expr],
    regular_param_count: usize,
) -> PreparedCallArgs {
    debug_assert!(sig.is_none() || !has_named_args(args_exprs));

    let is_variadic = sig.map(|s| s.variadic.is_some()).unwrap_or(false);

    let mut regular_args = Vec::new();
    let mut variadic_args = Vec::new();
    let mut spread_arg = None;
    let mut spread_at_index = 0usize;

    for (idx, arg) in args_exprs.iter().enumerate() {
        if let ExprKind::Spread(inner) = &arg.kind {
            spread_arg = Some((**inner).clone());
            spread_at_index = regular_args.len();
        } else if is_variadic && idx >= regular_param_count {
            variadic_args.push(arg.clone());
        } else {
            regular_args.push(arg.clone());
        }
    }

    let spread_into_named = spread_arg.is_some() && spread_at_index < regular_param_count;
    let mut all_args = regular_args;
    if !spread_into_named {
        if let Some(sig) = sig {
            for idx in all_args.len()..regular_param_count {
                if let Some(Some(default)) = sig.defaults.get(idx) {
                    all_args.push(default.clone());
                }
            }
        }
    }

    PreparedCallArgs {
        all_args,
        variadic_args,
        spread_arg,
        spread_at_index,
        regular_param_count,
        is_variadic,
        spread_into_named,
    }
}
