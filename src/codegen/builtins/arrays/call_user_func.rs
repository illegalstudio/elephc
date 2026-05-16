//! Purpose:
//! Emits PHP `call_user_func` builtin calls that invoke user-provided callbacks.
//! Owns callback argument materialization, result shape selection, and runtime helper calls.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Callback lowering must preserve PHP source evaluation order, captures, and callable return ownership.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::calls::args;
use crate::codegen::abi;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};
use super::callback_env;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("call_user_func()");
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    let call_reg = abi::nested_call_reg(emitter);

    // -- resolve callback function address --
    let is_callable_expr = matches!(
        &args[0].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    let precomputed_sig = crate::codegen::callables::callable_sig(&args[0], ctx);
    let captures =
        callback_env::materialize_callback_address(&args[0], call_reg, emitter, ctx, data);
    let sig: Option<FunctionSig> = if is_callable_expr {
        ctx.deferred_closures
            .last()
            .map(|deferred| deferred.sig.clone())
    } else {
        precomputed_sig
    };
    let ret_ty = sig
        .as_ref()
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Int);

    // -- evaluate remaining arguments and push onto stack --
    let mut arg_types = Vec::new();
    for (i, arg) in args[1..].iter().enumerate() {
        let is_ref = sig
            .as_ref()
            .and_then(|sig| sig.ref_params.get(i))
            .copied()
            .unwrap_or(false);
        let target_ty = args::declared_target_ty(sig.as_ref(), i);
        if is_ref {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if !args::emit_ref_arg_variable_address(var_name, "call_user_func ref arg", emitter, ctx) {
                    panic!("call_user_func() by-reference callback argument variable not found");
                }
            } else {
                panic!("call_user_func() by-reference callback argument must be a variable");
            }
            args::push_arg_value(emitter, &PhpType::Int);
            arg_types.push(PhpType::Int);
            continue;
        }

        let pushed_ty = args::push_expr_arg(arg, target_ty, emitter, ctx, data);
        arg_types.push(pushed_ty);
    }

    if let Some(sig) = &sig {
        let visible_param_count = sig.params.len();
        let regular_param_count = if sig.variadic.is_some() {
            visible_param_count.saturating_sub(1)
        } else {
            visible_param_count
        };
        for i in arg_types.len()..regular_param_count {
            if let Some(Some(default_expr)) = sig.defaults.get(i) {
                let target_ty = sig.params.get(i).map(|(_, ty)| ty);
                let pushed_ty = args::push_expr_arg(default_expr, target_ty, emitter, ctx, data);
                arg_types.push(pushed_ty);
            }
        }
    }
    callback_env::push_captures_as_hidden_args(&captures, emitter, ctx, &mut arg_types);

    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);

    // -- load callback address and call via blr --
    if !save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    abi::emit_call_reg(emitter, call_reg);
    if save_concat_before_args {
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    } else {
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
    }

    Some(ret_ty)
}
