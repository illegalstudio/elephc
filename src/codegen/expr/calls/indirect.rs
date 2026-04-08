use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::args;

pub(super) fn emit_expr_call(
    callee: &Expr,
    args_exprs: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("call expression result");

    let callee_sig = match &callee.kind {
        ExprKind::Variable(var_name) => ctx.closure_sigs.get(var_name).cloned(),
        ExprKind::ArrayAccess { array, .. } => {
            if let ExprKind::Variable(arr_name) = &array.kind {
                ctx.closure_sigs.get(arr_name).cloned()
            } else {
                None
            }
        }
        ExprKind::FirstClassCallable(target) => super::first_class_callable_sig(target, ctx),
        _ => None,
    };

    let is_variadic = callee_sig.as_ref().map(|s| s.variadic.is_some()).unwrap_or(false);
    let regular_param_count = callee_sig
        .as_ref()
        .map(|s| {
            if s.variadic.is_some() {
                s.params.len().saturating_sub(1)
            } else {
                s.params.len()
            }
        })
        .unwrap_or(args_exprs.len());
    let normalized_args = callee_sig
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
        if let Some(ref sig) = callee_sig {
            for i in all_args.len()..regular_param_count {
                if let Some(Some(default)) = sig.defaults.get(i) {
                    default_exprs.push(default.clone());
                }
            }
        }
        let default_refs: Vec<&Expr> = default_exprs.iter().collect();
        all_args.extend(default_refs);
    }

    let mut arg_types = Vec::new();
    for (i, arg) in all_args.iter().enumerate() {
        let is_ref = callee_sig
            .as_ref()
            .and_then(|sig| sig.ref_params.get(i))
            .copied()
            .unwrap_or(false);
        let target_ty = args::declared_target_ty(callee_sig.as_ref(), i);
        if is_ref {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if !args::emit_ref_arg_variable_address(var_name, "indirect ref arg", emitter, ctx) {
                    continue;
                }
            } else {
                let ty = super::super::emit_expr(arg, emitter, ctx, data);
                super::super::retain_borrowed_heap_arg(emitter, arg, &ty);
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
                callee_sig.as_ref(),
                spread_at_index,
                regular_param_count,
                "indirect params",
                emitter,
                ctx,
                data,
                &mut arg_types,
            );
        }
    }

    if is_variadic {
        if let Some(spread_expr) = spread_arg {
            let ty = args::emit_spread_variadic_array_arg(
                spread_expr,
                "spread array as indirect variadic param",
                emitter,
                ctx,
                data,
            );
            arg_types.push(ty);
        } else if variadic_args.is_empty() {
            arg_types.push(args::emit_empty_variadic_array_arg(
                "empty indirect variadic array",
                emitter,
            ));
        } else {
            arg_types.push(args::emit_variadic_array_arg_from_exprs(
                &variadic_args,
                "build indirect variadic array",
                true,
                true,
                emitter,
                ctx,
                data,
            ));
        }
    }

    let _callee_ty = super::super::emit_expr(callee, emitter, ctx, data);
    emitter.instruction("mov x9, x0");                                          // save closure address to x9
    emitter.instruction("str x9, [sp, #-16]!");                                 // push closure address temporarily

    let assignments = crate::codegen::abi::build_outgoing_arg_assignments(&arg_types, 0);

    emitter.instruction("ldr x9, [sp], #16");                                   // pop closure function address into x9
    let overflow_bytes = crate::codegen::abi::materialize_outgoing_args(emitter, &assignments);

    let ret_ty = callee_sig
        .as_ref()
        .map(|sig| sig.return_type.clone())
        .unwrap_or_else(|| match &callee.kind {
            ExprKind::Closure { body, .. } => crate::types::checker::infer_return_type_syntactic(body),
            _ => PhpType::Int,
        });

    emitter.instruction("mov x19, x9");                                         // preserve closure address across concat-offset save
    super::super::save_concat_offset_before_nested_call(emitter);
    emitter.instruction("blr x19");                                             // branch to closure via function pointer in x19
    super::super::restore_concat_offset_after_nested_call(emitter, &ret_ty);
    if overflow_bytes > 0 {
        emitter.instruction(&format!("add sp, sp, #{}", overflow_bytes));       // drop spilled stack arguments after the indirect call returns
    }

    ret_ty
}
