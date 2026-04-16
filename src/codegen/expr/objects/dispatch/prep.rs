use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::names::method_symbol;
use crate::parser::ast::{Expr, Visibility};
use crate::types::{FunctionSig, PhpType};

pub(super) fn eval_and_push_args(
    args: &[Expr],
    sig: Option<&FunctionSig>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<PhpType> {
    let prepared = super::super::super::calls::args::prepare_call_args(
        sig,
        args,
        super::super::super::calls::args::regular_param_count(sig, args.len()),
    );
    let mut arg_types = super::super::super::calls::args::emit_pushed_non_variadic_args(
        &prepared.all_args,
        sig,
        "method ref arg",
        true,
        emitter,
        ctx,
        data,
    );

    if prepared.spread_into_named {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            super::super::super::calls::args::emit_spread_into_named_params(
                spread_expr,
                sig,
                prepared.spread_at_index,
                prepared.regular_param_count,
                "method params",
                emitter,
                ctx,
                data,
                &mut arg_types,
            );
        }
    }

    if prepared.is_variadic {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            let ty = super::super::super::calls::args::emit_spread_variadic_array_arg(
                spread_expr,
                "spread array as variadic method param",
                emitter,
                ctx,
                data,
            );
            arg_types.push(ty);
        } else if prepared.variadic_args.is_empty() {
            arg_types.push(super::super::super::calls::args::emit_empty_variadic_array_arg(
                "empty variadic method array",
                emitter,
            ));
        } else {
            arg_types.push(super::super::super::calls::args::emit_variadic_array_arg_from_exprs(
                &prepared.variadic_args,
                "build variadic method array",
                true,
                true,
                emitter,
                ctx,
                data,
            ));
        }
    }
    arg_types
}

pub(super) fn compute_register_assignments(
    emitter: &Emitter,
    arg_types: &[PhpType],
    first_int_reg: usize,
) -> Vec<abi::OutgoingArgAssignment> {
    abi::build_outgoing_arg_assignments_for_target(emitter.target, arg_types, first_int_reg)
}

pub(super) fn pop_args_to_registers(
    emitter: &mut Emitter,
    assignments: &[abi::OutgoingArgAssignment],
) -> usize {
    abi::materialize_outgoing_args(emitter, assignments)
}

pub(super) fn resolve_instance_method_dispatch(
    ctx: &Context,
    class_name: &str,
    method: &str,
) -> (PhpType, Option<usize>, Option<String>) {
    let class_info = ctx.classes.get(class_name).cloned();
    let ret_ty = class_info
        .as_ref()
        .and_then(|ci| {
            let impl_class = ci
                .method_impl_classes
                .get(method)
                .map(String::as_str)
                .unwrap_or(class_name);
            ctx.classes
                .get(impl_class)
                .and_then(|impl_info| impl_info.methods.get(method))
                .cloned()
        })
        .map(|sig| sig.return_type)
        .unwrap_or(PhpType::Int);
    let slot = class_info
        .as_ref()
        .and_then(|ci| ci.vtable_slots.get(method).copied());
    let direct_private_label = class_info.as_ref().and_then(|ci| {
        if ci.method_visibilities.get(method) == Some(&Visibility::Private) {
            let impl_class = ci
                .method_impl_classes
                .get(method)
                .map(String::as_str)
                .unwrap_or(class_name);
            Some(method_symbol(impl_class, method))
        } else {
            None
        }
    });
    (ret_ty, slot, direct_private_label)
}
