//! Purpose:
//! Lowers top-level call-argument emission from prepared semantic plans.
//! Converts evaluated PHP argument expressions into temporary values ready for ABI assignment.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args`
//!
//! Key details:
//! - Argument checks must happen at PHP-observable points without skipping later side effects.

use crate::codegen::emit::Emitter;
use crate::codegen::{context::Context, data_section::DataSection};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

use super::common::{
    call_target_ty, emit_ref_arg_variable_address, push_arg_value,
    push_expr_arg, push_non_variable_ref_arg_address,
};
use super::named;
use super::normalize::{has_named_args, prepare_call_args};
use super::spread::{emit_spread_into_named_params, emit_spread_tail_variadic_array_arg};
use super::variadic::{emit_empty_variadic_array_arg, emit_variadic_array_arg_from_exprs};
use super::EmittedCallArgs;

pub(crate) fn emit_pushed_call_args(
    args_exprs: &[Expr],
    sig: Option<&FunctionSig>,
    regular_param_count: usize,
    ref_arg_context_label: &str,
    retain_non_variable_ref_args: bool,
    coerce_inferred_params: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> EmittedCallArgs {
    let has_named = has_named_args(args_exprs);

    if let Some(sig) = sig {
        if has_named {
            return named::emit_source_order_named_call_args(
                args_exprs,
                sig,
                regular_param_count,
                ref_arg_context_label,
                retain_non_variable_ref_args,
                emitter,
                ctx,
                data,
            );
        }
    }

    debug_assert!(
        sig.is_some() || !has_named,
        "codegen reached named-arg call without a known signature; checker should have rejected this"
    );

    let prepared = prepare_call_args(sig, args_exprs, regular_param_count);
    let mut arg_types = emit_pushed_non_variadic_args(
        &prepared.all_args,
        sig,
        ref_arg_context_label,
        retain_non_variable_ref_args,
        coerce_inferred_params,
        emitter,
        ctx,
        data,
    );

    if prepared.spread_into_named {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            emit_spread_into_named_params(
                spread_expr,
                sig,
                prepared.spread_at_index,
                prepared.regular_param_count,
                "named params",
                emitter,
                ctx,
                data,
                &mut arg_types,
            );
        }
    }

    if prepared.is_variadic {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            let tail_start = prepared
                .regular_param_count
                .saturating_sub(prepared.spread_at_index);
            let variadic_ty = emit_spread_tail_variadic_array_arg(
                spread_expr,
                tail_start,
                "spread tail as variadic param",
                emitter,
                ctx,
                data,
            );
            arg_types.push(variadic_ty);
        } else if prepared.variadic_args.is_empty() {
            arg_types.push(emit_empty_variadic_array_arg("empty variadic array", emitter));
        } else {
            let variadic_ty = emit_variadic_array_arg_from_exprs(
                &prepared.variadic_args,
                "build variadic array",
                true,
                true,
                emitter,
                ctx,
                data,
            );
            arg_types.push(variadic_ty);
        }
    }

    EmittedCallArgs {
        arg_types,
        source_temp_bytes: 0,
    }
}

pub(crate) fn emit_pushed_non_variadic_args(
    all_args: &[Expr],
    sig: Option<&FunctionSig>,
    ref_arg_context_label: &str,
    _retain_non_variable_ref_args: bool,
    coerce_inferred_params: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<PhpType> {
    let mut arg_types = Vec::new();

    for (idx, arg) in all_args.iter().enumerate() {
        let is_ref = sig
            .and_then(|sig| sig.ref_params.get(idx))
            .copied()
            .unwrap_or(false);
        let target_ty = call_target_ty(sig, idx, coerce_inferred_params);

        if is_ref {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if !emit_ref_arg_variable_address(var_name, ref_arg_context_label, emitter, ctx) {
                    continue;
                }
                push_arg_value(emitter, &PhpType::Int);
            } else {
                push_non_variable_ref_arg_address(arg, target_ty, emitter, ctx, data);
            }
            arg_types.push(PhpType::Int);
        } else {
            let pushed_ty = push_expr_arg(arg, target_ty, emitter, ctx, data);
            arg_types.push(pushed_ty);
        }
    }

    arg_types
}
