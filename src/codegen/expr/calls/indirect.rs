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

    let prepared = args::prepare_call_args(
        callee_sig.as_ref(),
        args_exprs,
        args::regular_param_count(callee_sig.as_ref(), args_exprs.len()),
    );
    let mut arg_types = args::emit_pushed_non_variadic_args(
        &prepared.all_args,
        callee_sig.as_ref(),
        "indirect ref arg",
        true,
        emitter,
        ctx,
        data,
    );

    if prepared.spread_into_named {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            args::emit_spread_into_named_params(
                spread_expr,
                callee_sig.as_ref(),
                prepared.spread_at_index,
                prepared.regular_param_count,
                "indirect params",
                emitter,
                ctx,
                data,
                &mut arg_types,
            );
        }
    }

    if prepared.is_variadic {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            let ty = args::emit_spread_variadic_array_arg(
                spread_expr,
                "spread array as indirect variadic param",
                emitter,
                ctx,
                data,
            );
            arg_types.push(ty);
        } else if prepared.variadic_args.is_empty() {
            arg_types.push(args::emit_empty_variadic_array_arg(
                "empty indirect variadic array",
                emitter,
            ));
        } else {
            arg_types.push(args::emit_variadic_array_arg_from_exprs(
                &prepared.variadic_args,
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
