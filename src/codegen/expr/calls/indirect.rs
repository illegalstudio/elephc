//! Purpose:
//! Lowers variable and callable-indirect invocation paths.
//! Resolves the callable shape, prepares arguments, and leaves the call result for expression consumers.
//!
//! Called from:
//! - `crate::codegen::expr::calls`
//!
//! Key details:
//! - Callable metadata and argument signatures must stay synchronized with type checking and runtime dispatch.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::args;

pub(super) fn emit_loaded_expr_call(
    callee: &Expr,
    args_exprs: &[Expr],
    _loaded_callee_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("call loaded expression result");
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        super::super::save_concat_offset_before_nested_call(emitter, ctx);
    }

    let callee_sig = callee_sig_for_expr(callee, ctx);
    let captures = crate::codegen::callables::callable_captures(callee, ctx);
    crate::codegen::abi::emit_push_reg(emitter, crate::codegen::abi::int_result_reg(emitter)); // save the already-evaluated callable below later arguments

    let emitted_args = args::emit_pushed_call_args(
        args_exprs,
        callee_sig.as_ref(),
        args::regular_param_count(callee_sig.as_ref(), args_exprs.len()),
        "indirect ref arg",
        true,
        emitter,
        ctx,
        data,
    );
    let mut arg_types = emitted_args.arg_types;

    let call_reg = crate::codegen::abi::nested_call_reg(emitter);
    let arg_temp_bytes = args::pushed_temp_bytes(&arg_types) + emitted_args.source_temp_bytes;
    crate::codegen::abi::emit_load_temporary_stack_slot(emitter, call_reg, arg_temp_bytes);
    push_captures_as_hidden_args(&captures, emitter, ctx, &mut arg_types);

    let assignments =
        crate::codegen::abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    let overflow_bytes = crate::codegen::abi::materialize_outgoing_args(emitter, &assignments);

    let ret_ty = callee_sig
        .as_ref()
        .map(|sig| sig.return_type.clone())
        .unwrap_or_else(|| match &callee.kind {
            ExprKind::Closure {
                return_type: Some(type_ann),
                ..
            } => crate::codegen::functions::codegen_static_type(type_ann, ctx),
            ExprKind::Closure { body, .. } => {
                crate::types::checker::infer_return_type_syntactic(body)
            }
            _ => PhpType::Int,
        });

    if save_concat_before_args {
        crate::codegen::abi::emit_call_reg(emitter, call_reg);
        crate::codegen::abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
        crate::codegen::abi::emit_release_temporary_stack(emitter, 16);
        super::super::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    } else {
        super::super::save_concat_offset_before_nested_call(emitter, ctx);
        crate::codegen::abi::emit_call_reg(emitter, call_reg);
        super::super::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        crate::codegen::abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
        crate::codegen::abi::emit_release_temporary_stack(emitter, 16);
    }

    ret_ty
}

fn callee_sig_for_expr(
    callee: &Expr,
    ctx: &Context,
) -> Option<crate::types::FunctionSig> {
    match &callee.kind {
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
    }
}

fn push_captures_as_hidden_args(
    captures: &[(String, PhpType)],
    emitter: &mut Emitter,
    ctx: &Context,
    arg_types: &mut Vec<PhpType>,
) {
    for (capture_name, capture_ty) in captures {
        emitter.comment(&format!("push callable capture ${}", capture_name));
        let Some(capture_info) = ctx.variables.get(capture_name) else {
            emitter.comment(&format!(
                "WARNING: captured callable variable ${} not found",
                capture_name
            ));
            continue;
        };
        crate::codegen::abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
        args::push_arg_value(emitter, capture_ty);
        arg_types.push(capture_ty.clone());
    }
}
