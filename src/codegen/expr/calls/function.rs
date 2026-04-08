use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::names::function_symbol;
use crate::parser::ast::Expr;
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
    let prepared = args::prepare_call_args(
        sig.as_ref(),
        args_exprs,
        args::regular_param_count(sig.as_ref(), args_exprs.len()),
    );
    let mut arg_types = args::emit_pushed_non_variadic_args(
        &prepared.all_args,
        sig.as_ref(),
        "ref arg",
        false,
        emitter,
        ctx,
        data,
    );

    if prepared.spread_into_named {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            args::emit_spread_into_named_params(
                spread_expr,
                sig.as_ref(),
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
            let _ = args::emit_spread_variadic_array_arg(
                spread_expr,
                "spread array as variadic param",
                emitter,
                ctx,
                data,
            );
        } else if prepared.variadic_args.is_empty() {
            let _ = args::emit_empty_variadic_array_arg("empty variadic array", emitter);
        } else {
            let _ = args::emit_variadic_array_arg_from_exprs(
                &prepared.variadic_args,
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

    let assignments =
        crate::codegen::abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
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
