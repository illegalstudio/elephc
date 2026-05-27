//! Purpose:
//! Emits PHP `call_user_func` builtin calls that invoke user-provided callbacks.
//! Owns callback argument materialization, result shape selection, and runtime helper calls.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Callback lowering must preserve PHP source evaluation order, captures, and callable return ownership.

use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{emit_expr, expr_result_heap_ownership};
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
    if let Some(ret_ty) = emit_descriptor_backed_call_user_func(
        &args[0],
        &args[1..],
        call_reg,
        save_concat_before_args,
        emitter,
        ctx,
        data,
    ) {
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

/// Emits descriptor-invoker dispatch for callable values already represented as descriptors.
///
/// Variable arguments are encoded as invoker-only reference-cell markers when
/// the signature has by-reference slots, or when the static signature is not
/// known and the generated descriptor invoker must decide from runtime metadata.
/// Captures, including by-ref captures, remain descriptor-owned and are loaded
/// by the generated invoker.
#[allow(clippy::too_many_arguments)]
fn emit_descriptor_backed_call_user_func(
    callback: &Expr,
    callback_args: &[Expr],
    call_reg: &str,
    concat_saved_before_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let sig = descriptor_invoker_sig(callback, ctx)?;
    let ownership = expr_result_heap_ownership(callback);
    if !matches!(ownership, HeapOwnership::Owned | HeapOwnership::Borrowed) {
        return None;
    }

    let _callback_ty = emit_expr(callback, emitter, ctx, data);
    if matches!(ownership, HeapOwnership::Borrowed) {
        crate::codegen::callable_descriptor::emit_retain_current_descriptor(emitter);
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the callable descriptor while building call_user_func() arguments

    let encode_variable_refs = should_encode_invoker_ref_args(sig.as_ref(), callback_args);
    if let Some(sig) = sig.as_ref() {
        validate_descriptor_call_user_func_ref_args(sig, callback_args);
    }
    let arr_ty = if encode_variable_refs {
        emit_descriptor_invoker_arg_array_for_call_user_func(
            callback_args,
            encode_variable_refs,
            emitter,
            ctx,
            data,
        )
    } else {
        let arg_array = Expr::new(
            ExprKind::ArrayLiteral(callback_args.to_vec()),
            callback.span,
        );
        emit_expr(&arg_array, emitter, ctx, data)
    };
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the owned call_user_func() argument array for invocation and cleanup
    abi::emit_load_temporary_stack_slot(emitter, call_reg, 16);
    call_user_func_array::emit_call_descriptor_array_invoker(
        call_user_func_array::LoadedArraySource::TemporaryStackSlot(0),
        &arr_ty,
        call_reg,
        concat_saved_before_args,
        emitter,
        ctx,
        data,
    );
    release_owned_arg_array_after_mixed_result(&arr_ty, emitter);
    release_preserved_descriptor_after_mixed_result(emitter);
    Some(PhpType::Mixed)
}

/// Returns whether call_user_func() should encode variable args as ref-cell markers.
fn should_encode_invoker_ref_args(sig: Option<&FunctionSig>, callback_args: &[Expr]) -> bool {
    if !callback_args
        .iter()
        .any(|arg| matches!(arg.kind, ExprKind::Variable(_)))
    {
        return false;
    }
    sig.is_none_or(|sig| sig.ref_params.iter().any(|is_ref| *is_ref))
}

/// Preserves PHP's explicit by-reference argument rule for statically known callbacks.
fn validate_descriptor_call_user_func_ref_args(sig: &FunctionSig, callback_args: &[Expr]) {
    for (i, arg) in callback_args.iter().enumerate() {
        if sig.ref_params.get(i).copied().unwrap_or(false)
            && !matches!(arg.kind, ExprKind::Variable(_))
        {
            panic!("call_user_func() by-reference callback argument must be a variable");
        }
    }
}

/// Builds the descriptor-invoker argument array used by call_user_func().
fn emit_descriptor_invoker_arg_array_for_call_user_func(
    callback_args: &[Expr],
    encode_variable_refs: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("call_user_func() descriptor argument array");
    let capacity = callback_args.len().max(4);
    let capacity_reg = abi::int_arg_reg_name(emitter.target, 0);
    let elem_size_reg = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_int_immediate(emitter, capacity_reg, capacity as i64);
    abi::emit_load_int_immediate(emitter, elem_size_reg, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // keep the descriptor argument array alive while filling Mixed slots
    abi::emit_load_temporary_stack_slot(emitter, abi::symbol_scratch_reg(emitter), 0);
    crate::codegen::expr::arrays::emit_array_value_type_stamp(
        emitter,
        abi::symbol_scratch_reg(emitter),
        &PhpType::Mixed,
    );

    for (i, arg) in callback_args.iter().enumerate() {
        if encode_variable_refs {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if !args::emit_ref_arg_variable_address(
                    var_name,
                    "call_user_func descriptor arg",
                    emitter,
                    ctx,
                ) {
                    panic!("call_user_func() descriptor argument variable not found");
                }
                emit_box_current_ref_arg_address_for_invoker(var_name, emitter, ctx);
                emit_store_descriptor_invoker_arg_array_slot(i, emitter);
                continue;
            }
        }

        let mut ty = emit_expr(arg, emitter, ctx, data);
        let boxed_iterable = crate::codegen::emit_box_iterable_value_for_mixed_container(
            emitter,
            &mut ty,
        );
        if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            crate::codegen::emit_box_current_expr_value_as_mixed_for_container(emitter, arg, &ty);
        } else if !boxed_iterable {
            retain_borrowed_mixed_arg_for_invoker(emitter, arg, &ty);
        }
        emit_store_descriptor_invoker_arg_array_slot(i, emitter);
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // return the filled descriptor argument array
    PhpType::Array(Box::new(PhpType::Mixed))
}

/// Retains a borrowed boxed Mixed argument before storing it in the invoker array.
fn retain_borrowed_mixed_arg_for_invoker(emitter: &mut Emitter, arg: &Expr, ty: &PhpType) {
    if ty.codegen_repr().is_refcounted() && expr_result_heap_ownership(arg) != HeapOwnership::Owned {
        abi::emit_incref_if_refcounted(emitter, &ty.codegen_repr());
    }
}

/// Boxes the current variable storage address as an invoker-only Mixed marker.
fn emit_box_current_ref_arg_address_for_invoker(
    var_name: &str,
    emitter: &mut Emitter,
    ctx: &Context,
) {
    let ref_cell_reg = abi::secondary_scratch_reg(emitter);
    let marker_tag_reg = abi::tertiary_scratch_reg(emitter);
    let source_tag_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", ref_cell_reg, abi::int_result_reg(emitter))); // preserve the source variable storage address before Mixed marker boxing
    abi::emit_load_int_immediate(
        emitter,
        marker_tag_reg,
        call_user_func_array::INVOKER_ARG_REF_CELL_TAG,
    );
    abi::emit_load_int_immediate(
        emitter,
        source_tag_reg,
        variable_runtime_value_tag(var_name, ctx) as i64,
    );
    crate::codegen::emit_box_runtime_payload_as_mixed(
        emitter,
        marker_tag_reg,
        ref_cell_reg,
        source_tag_reg,
    );
}

/// Returns the runtime tag for a variable's current codegen type.
fn variable_runtime_value_tag(var_name: &str, ctx: &Context) -> u8 {
    ctx.variables
        .get(var_name)
        .map(|var| crate::codegen::runtime_value_tag(&var.ty.codegen_repr()))
        .unwrap_or_else(|| crate::codegen::runtime_value_tag(&PhpType::Int))
}

/// Stores the current boxed Mixed argument into the synthetic invoker array.
fn emit_store_descriptor_invoker_arg_array_slot(index: usize, emitter: &mut Emitter) {
    let array_reg = abi::symbol_scratch_reg(emitter);
    let len_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, array_reg, 0);
    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), array_reg, 24 + index * 8);
    abi::emit_load_int_immediate(emitter, len_reg, (index + 1) as i64);
    abi::emit_store_to_address(emitter, len_reg, array_reg, 0);
}

/// Releases the synthetic call_user_func() argument array while preserving the call result.
fn release_owned_arg_array_after_mixed_result(arr_ty: &PhpType, emitter: &mut Emitter) {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed call result while releasing the synthetic argument array
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, arr_ty);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the boxed call result after argument-array cleanup
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the preserved synthetic argument-array slot
}

/// Returns optional callable signature metadata for expressions that produce descriptor values.
fn descriptor_invoker_sig(callback: &Expr, ctx: &Context) -> Option<Option<FunctionSig>> {
    if matches!(callback.kind, ExprKind::StringLiteral(_) | ExprKind::Closure { .. }) {
        return None;
    }
    if matches!(&callback.kind, ExprKind::Variable(name) if ctx.ref_params.contains(name)) {
        return None;
    }
    match &callback.kind {
        ExprKind::Variable(_)
        | ExprKind::ArrayAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::FunctionCall { .. }
        | ExprKind::Assignment { .. }
        | ExprKind::Ternary { .. }
        | ExprKind::ShortTernary { .. }
        | ExprKind::NullCoalesce { .. } => {}
        _ => return None,
    }
    let static_sig = crate::codegen::callables::callable_sig(callback, ctx);
    if static_sig.is_some()
        || matches!(
            crate::codegen::functions::infer_contextual_type(callback, ctx).codegen_repr(),
            PhpType::Callable
        )
    {
        Some(static_sig)
    } else {
        None
    }
}

/// Releases the preserved callback descriptor while keeping the boxed Mixed call result live.
fn release_preserved_descriptor_after_mixed_result(emitter: &mut Emitter) {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve call_user_func() result while releasing the callback descriptor owner
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    crate::codegen::callable_descriptor::emit_release_current_descriptor(emitter);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the boxed Mixed call_user_func() result
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the preserved callable descriptor slot
}
