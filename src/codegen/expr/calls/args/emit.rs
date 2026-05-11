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
use crate::types::call_args;
use crate::types::{FunctionSig, PhpType};

use super::common::{declared_target_ty, emit_ref_arg_variable_address, push_arg_value, push_expr_arg};
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
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> EmittedCallArgs {
    let expanded_args = call_args::expand_static_assoc_spread_args(args_exprs);
    let args_exprs = expanded_args.as_slice();
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
    retain_non_variable_ref_args: bool,
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
        let target_ty = declared_target_ty(sig, idx);

        if is_ref {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if !emit_ref_arg_variable_address(var_name, ref_arg_context_label, emitter, ctx) {
                    continue;
                }
            } else {
                let source_ty = super::super::super::emit_expr(arg, emitter, ctx, data);
                if retain_non_variable_ref_args {
                    super::super::super::retain_borrowed_heap_arg(emitter, arg, &source_ty);
                }
            }
            push_arg_value(emitter, &PhpType::Int);
            arg_types.push(PhpType::Int);
        } else {
            let pushed_ty = push_expr_arg(arg, target_ty, emitter, ctx, data);
            arg_types.push(pushed_ty);
        }
    }

    arg_types
}
