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
use super::callable_forms;
use super::call_user_func_array;
use super::super::callable_lookup::{lookup_function, FunctionLookup};

/// Emits `call_user_func($callback, ...$args)` builtin calls.
///
/// Dispatches to extern/builtin when the first argument is a string literal known
/// at compile time. Otherwise, materializes the callback address, evaluates all
/// remaining arguments in PHP source order (including by-reference and default
/// parameter padding), pushes captures as hidden arguments, then emits the call
/// via `blr`.
///
/// Arguments:
/// - `args[0]`: callback (string literal function name, closure, or first-class callable)
/// - `args[1..]`: arguments to pass through to the callback
///
/// Returns the inferred return type from the callback's signature, defaulting to `Int`
/// when the signature cannot be determined.
///
/// ABI constraints:
/// - Callback address is placed in `call_reg` before `blr`.
/// - Arguments are materialized as outgoing args via `materialize_outgoing_args`.
/// - On x86_64, concat offset is saved/restored around the nested call.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("call_user_func()");
    if let ExprKind::StringLiteral(name) = &args[0].kind {
        match lookup_function(ctx, name) {
            Some(FunctionLookup::Extern(extern_name)) => {
                return Some(crate::codegen::ffi::emit_extern_call(
                    &extern_name,
                    &args[1..],
                    args[0].span,
                    emitter,
                    ctx,
                    data,
                ));
            }
            Some(FunctionLookup::Builtin(builtin_name)) => {
                if let Some(ret_ty) = crate::codegen::builtins::emit_builtin_call(
                    &builtin_name,
                    &args[1..],
                    args[0].span,
                    emitter,
                    ctx,
                    data,
                ) {
                    return Some(ret_ty);
                }
            }
            Some(FunctionLookup::UserFunction(_)) | Some(FunctionLookup::IncludeVariant(_)) | None => {}
        }
    }
    if let Some(ret_ty) = callable_forms::emit_call_user_func_form(
        &args[0],
        &args[1..],
        emitter,
        ctx,
        data,
    ) {
        return Some(ret_ty);
    }
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    let call_reg = abi::nested_call_reg(emitter);
    if call_user_func_array::callback_is_runtime_string(&args[0], ctx) {
        let arg_array = Expr::new(
            ExprKind::ArrayLiteral(args[1..].to_vec()),
            args[0].span,
        );
        let ret_ty = call_user_func_array::emit_dynamic_string_callback_with_array_expr(
            &args[0],
            &arg_array,
            call_reg,
            save_concat_before_args,
            emitter,
            ctx,
            data,
        );
        return Some(ret_ty);
    }

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
