use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::args;

pub(super) fn emit_function_call(
    name: &str,
    args_exprs: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("call {}()", name));

    let sig = ctx.functions.get(name).cloned();
    let is_variadic = sig.as_ref().map(|s| s.variadic.is_some()).unwrap_or(false);

    let regular_param_count = if is_variadic {
        sig.as_ref()
            .map(|s| s.params.len().saturating_sub(1))
            .unwrap_or(0)
    } else {
        sig.as_ref().map(|s| s.params.len()).unwrap_or(args_exprs.len())
    };
    let normalized_args = sig
        .as_ref()
        .map(|sig| args::normalize_named_call_args(sig, args_exprs, regular_param_count))
        .unwrap_or_else(|| args_exprs.to_vec());
    let args_exprs = normalized_args.as_slice();

    let mut regular_args: Vec<&Expr> = Vec::new();
    let mut variadic_args: Vec<&Expr> = Vec::new();
    let mut spread_arg: Option<&Expr> = None;
    let mut spread_at_index: usize = 0;

    for (i, arg) in args_exprs.iter().enumerate() {
        if let ExprKind::Spread(inner) = &arg.kind {
            spread_arg = Some(inner.as_ref());
            spread_at_index = regular_args.len();
        } else if is_variadic && i >= regular_param_count {
            variadic_args.push(arg);
        } else {
            regular_args.push(arg);
        }
    }

    let spread_into_named = spread_arg.is_some() && !is_variadic;

    let mut all_args: Vec<&Expr> = regular_args;
    let mut default_exprs: Vec<Expr> = Vec::new();

    if !spread_into_named {
        if let Some(ref s) = sig {
            for i in all_args.len()..regular_param_count {
                if let Some(Some(default)) = s.defaults.get(i) {
                    default_exprs.push(default.clone());
                }
            }
        }
        let default_refs: Vec<&Expr> = default_exprs.iter().collect();
        all_args.extend(default_refs);
    }

    let ref_params = sig
        .as_ref()
        .map(|s| s.ref_params.clone())
        .unwrap_or_default();

    let mut arg_types = Vec::new();
    for (i, arg) in all_args.iter().enumerate() {
        let is_ref = ref_params.get(i).copied().unwrap_or(false);
        let target_ty = args::declared_target_ty(sig.as_ref(), i);
        if is_ref {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if !args::emit_ref_arg_variable_address(var_name, "ref arg", emitter, ctx) {
                    continue;
                }
            } else {
                super::super::emit_expr(arg, emitter, ctx, data);
            }
            args::push_arg_value(emitter, &PhpType::Int);
            arg_types.push(PhpType::Int);
        } else {
            let pushed_ty = args::push_expr_arg(arg, target_ty, emitter, ctx, data);
            arg_types.push(pushed_ty);
        }
    }

    if spread_into_named {
        if let Some(spread_expr) = spread_arg {
            args::emit_spread_into_named_params(
                spread_expr,
                sig.as_ref(),
                spread_at_index,
                regular_param_count,
                "named params",
                emitter,
                ctx,
                data,
                &mut arg_types,
            );
        }
    }

    if is_variadic {
        if let Some(spread_expr) = spread_arg {
            let _ = args::emit_spread_variadic_array_arg(
                spread_expr,
                "spread array as variadic param",
                emitter,
                ctx,
                data,
            );
        } else if variadic_args.is_empty() {
            let _ = args::emit_empty_variadic_array_arg("empty variadic array", emitter);
        } else {
            let _ = args::emit_variadic_array_arg_from_exprs(
                &variadic_args,
                "build variadic array",
                false,
                false,
                emitter,
                ctx,
                data,
            );
        }
        arg_types.push(PhpType::Array(Box::new(PhpType::Int)));
    }

    let assignments = crate::codegen::abi::build_outgoing_arg_assignments(&arg_types, 0);
    let overflow_bytes = crate::codegen::abi::materialize_outgoing_args(emitter, &assignments);

    let ret_ty = ctx
        .functions
        .get(name)
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Void);

    super::super::save_concat_offset_before_nested_call(emitter);
    emitter.instruction(&format!("bl {}", function_symbol(name)));              // branch-and-link to compiled PHP function
    super::super::restore_concat_offset_after_nested_call(emitter, &ret_ty);
    if overflow_bytes > 0 {
        emitter.instruction(&format!("add sp, sp, #{}", overflow_bytes));       // drop spilled stack arguments after the nested call returns
    }

    ret_ty
}
