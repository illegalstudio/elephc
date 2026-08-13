//! Purpose:
//! Lowers interface, nullable, and runtime-backed intrinsic method calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers an instance-method call through interface metadata.
pub(super) fn lower_interface_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    interface_name: &str,
    method_name: &str,
) -> Result<()> {
    // Builtin Throwable methods are compact-payload intrinsics. Their interface vtable
    // slots stay zero because no synthetic method bodies are emitted, so dispatch here
    // would `blr` to null. Use the same intrinsic path as a concrete Throwable receiver.
    if is_throwable_standard_method_call(ctx, interface_name, method_name) {
        let object = expect_operand(inst, 0)?;
        return lower_throwable_standard_method(ctx, inst, object, method_name);
    }
    let (normalized, method_key, callee_sig) =
        resolve_interface_call_signature(ctx, interface_name, method_name, inst.operands.len())?;
    let mut param_types = Vec::with_capacity(callee_sig.params.len() + 1);
    param_types.push(PhpType::Object(normalized.clone()));
    param_types.extend(callee_sig.params.iter().map(|(_, ty)| ty.codegen_repr()));
    let mut ref_params = Vec::with_capacity(callee_sig.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(callee_sig.ref_params.iter().copied());
    let call_args = materialize_direct_call_args_with_refs_and_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
        RefArgCellLifetime::CallOnly,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    let return_ty = iterators::emit_interface_dispatch_call(ctx, &normalized, &method_key, None)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &return_ty)?;
    emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Resolves interface method metadata and validates the EIR ABI operand count.
pub(super) fn resolve_interface_call_signature(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    method_name: &str,
    operand_count: usize,
) -> Result<(String, String, FunctionSig)> {
    let normalized = interface_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method_name);
    let callee_sig = ctx
        .module
        .interface_infos
        .get(normalized)
        .and_then(|interface_info| interface_info.methods.get(&method_key))
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "interface method call to unknown method {}::{}",
                normalized, method_name
            ))
        })?
        .clone();
    let expected_args = callee_sig.params.len() + 1;
    if operand_count != expected_args {
        return Err(CodegenIrError::unsupported(format!(
            "interface method call to {}::{} with {} operands for {} ABI params",
            normalized, method_name, operand_count, expected_args
        )));
    }
    Ok((normalized.to_string(), method_key, callee_sig))
}

/// Lowers a method call after an earlier EIR guard has proven a nullable receiver non-null.
pub(super) fn lower_nullable_receiver_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: &str,
    method_name: &str,
) -> Result<()> {
    if ctx
        .module
        .interface_infos
        .contains_key(class_name.trim_start_matches('\\'))
    {
        return lower_nullable_receiver_interface_method_call(
            ctx,
            inst,
            object,
            class_name,
            method_name,
        );
    }
    let target = resolve_method_call_target(ctx, class_name, method_name, inst.operands.len())?;
    let receiver_ty = PhpType::Object(class_name.to_string());
    let mut param_types = Vec::with_capacity(target.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(target.ref_params.iter().copied());
    let null_label = ctx.next_label("method_receiver_null");
    let done_label = ctx.next_label("method_receiver_done");
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    objects::emit_nullable_receiver_object_payload(ctx, object, &null_label, receiver_reg)?;
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    if let Some(slot) = target.dynamic_slot {
        emit_dynamic_instance_method_call(ctx, slot);
    } else {
        abi::emit_call_label(
            ctx.emitter,
            &method_symbol(&target.impl_class, &target.method_key),
        );
    }
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_method_call_result(ctx, inst, &target)?;
    emit_ref_arg_writebacks(ctx, &call_args)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_method_call_on_null_fatal(ctx, method_name);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a nullable receiver call whose non-null payload is known only by interface type.
pub(super) fn lower_nullable_receiver_interface_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    interface_name: &str,
    method_name: &str,
) -> Result<()> {
    // Same compact-payload intrinsic path as `lower_interface_method_call`: Throwable's
    // interface vtable slots are empty, so nullable `?Throwable` must not dispatch through them.
    if is_throwable_standard_method_call(ctx, interface_name, method_name) {
        let null_label = ctx.next_label("method_receiver_null");
        let done_label = ctx.next_label("method_receiver_done");
        let receiver_reg = abi::nested_call_reg(ctx.emitter);
        objects::emit_nullable_receiver_object_payload(ctx, object, &null_label, receiver_reg)?;
        // Re-materialize the unboxed object into the SSA operand's result register so the
        // shared Throwable intrinsic lowerer can `load_value_to_reg` the receiver.
        lower_throwable_standard_method_from_reg(ctx, inst, receiver_reg, method_name)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&null_label);
        emit_method_call_on_null_fatal(ctx, method_name);
        ctx.emitter.label(&done_label);
        return Ok(());
    }
    let normalized = interface_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method_name);
    let callee_sig = ctx
        .module
        .interface_infos
        .get(normalized)
        .and_then(|interface_info| interface_info.methods.get(&method_key))
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "interface method call to unknown method {}::{}",
                normalized, method_name
            ))
        })?
        .clone();
    let expected_args = callee_sig.params.len() + 1;
    if inst.operands.len() != expected_args {
        return Err(CodegenIrError::unsupported(format!(
            "interface method call to {}::{} with {} operands for {} ABI params",
            normalized,
            method_name,
            inst.operands.len(),
            expected_args
        )));
    }
    let receiver_ty = PhpType::Object(normalized.to_string());
    let mut param_types = Vec::with_capacity(callee_sig.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(callee_sig.params.iter().map(|(_, ty)| ty.codegen_repr()));
    let mut ref_params = Vec::with_capacity(callee_sig.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(callee_sig.ref_params.iter().copied());
    let null_label = ctx.next_label("method_receiver_null");
    let done_label = ctx.next_label("method_receiver_done");
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    objects::emit_nullable_receiver_object_payload(ctx, object, &null_label, receiver_reg)?;
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    let return_ty = iterators::emit_interface_dispatch_call(ctx, normalized, &method_key, None)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &return_ty)?;
    emit_ref_arg_writebacks(ctx, &call_args)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_method_call_on_null_fatal(ctx, method_name);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits PHP's fatal diagnostic for calling an instance method on null.
pub(super) fn emit_method_call_on_null_fatal(ctx: &mut FunctionContext<'_>, method_name: &str) {
    exceptions::emit_error(
        ctx,
        &format!("Call to a member function {}() on null", method_name),
    );
}

/// Returns the direct runtime intrinsic for built-in `Generator` instance methods.
pub(super) fn generator_intrinsic(class_name: &str, method_name: &str) -> Option<IntrinsicCall> {
    if class_name.trim_start_matches('\\') != "Generator" {
        return None;
    }
    IntrinsicCall::instance_method("Generator", method_name)
}

/// Returns the descriptor-backed intrinsic for SPL callback-filter accept trampolines.
pub(super) fn callback_filter_intrinsic(class_name: &str, method_name: &str) -> Option<IntrinsicCall> {
    let intrinsic =
        IntrinsicCall::instance_method(class_name.trim_start_matches('\\'), method_name)?;
    if intrinsic.kind() == IntrinsicCallKind::CallbackFilterAccept {
        Some(intrinsic)
    } else {
        None
    }
}

/// Returns a runtime-backed intrinsic for ordinary direct instance-method calls.
pub(super) fn runtime_backed_instance_intrinsic(class_name: &str, method_name: &str) -> Option<IntrinsicCall> {
    let intrinsic =
        IntrinsicCall::instance_method(class_name.trim_start_matches('\\'), method_name)?;
    intrinsic.runtime_helper()?;
    Some(intrinsic)
}

/// Returns a runtime-backed intrinsic for ordinary direct static-method calls.
pub(super) fn runtime_backed_static_intrinsic(class_name: &str, method_name: &str) -> Option<IntrinsicCall> {
    let intrinsic = IntrinsicCall::static_method(class_name.trim_start_matches('\\'), method_name)?;
    intrinsic.runtime_helper()?;
    Some(intrinsic)
}

/// Lowers a runtime-backed intrinsic instance method using normal method ABI arguments.
pub(super) fn lower_instance_runtime_intrinsic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    method_name: &str,
    intrinsic: IntrinsicCall,
) -> Result<()> {
    let normalized = class_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method_name);
    let class_info = ctx.module.class_infos.get(normalized).ok_or_else(|| {
        CodegenIrError::unsupported(format!("intrinsic method on unknown class {}", normalized))
    })?;
    let callee_sig = class_info.methods.get(&method_key).ok_or_else(|| {
        CodegenIrError::unsupported(format!("intrinsic method {}::{}", normalized, method_name))
    })?;
    let expected_args = callee_sig.params.len() + 1;
    if inst.operands.len() != expected_args {
        return Err(CodegenIrError::unsupported(format!(
            "intrinsic method call to {}::{} with {} operands for {} ABI params",
            normalized,
            method_name,
            inst.operands.len(),
            expected_args
        )));
    }
    let return_ty = callee_sig.return_type.clone();
    let callee_params = callee_sig.params.clone();
    let callee_ref_params = callee_sig.ref_params.clone();
    let mut param_types = Vec::with_capacity(callee_params.len() + 1);
    param_types.push(PhpType::Object(normalized.to_string()));
    param_types.extend(callee_params.iter().map(|(_, ty)| ty.codegen_repr()));
    let mut ref_params = Vec::with_capacity(callee_ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(callee_ref_params.iter().copied());
    let call_args =
        materialize_direct_call_args_with_refs(ctx, &inst.operands, &param_types, &ref_params)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(
        ctx.emitter,
        intrinsic
            .runtime_helper()
            .expect("runtime-backed instance intrinsic must have a helper"),
    );
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &return_ty)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Lowers a runtime-backed intrinsic static method using the hidden called-class id ABI.
pub(super) fn lower_static_runtime_intrinsic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver: &str,
    method_name: &str,
    called_class_id: &CalledClassIdArg,
    intrinsic: IntrinsicCall,
) -> Result<()> {
    let method_key = php_symbol_key(method_name);
    let receiver_info = ctx.module.class_infos.get(receiver).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "intrinsic static method on unknown class {}",
            receiver
        ))
    })?;
    let callee_sig = receiver_info
        .static_methods
        .get(&method_key)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "intrinsic static method {}::{}",
                receiver, method_name
            ))
        })?;
    if inst.operands.len() != callee_sig.params.len() {
        return Err(CodegenIrError::unsupported(format!(
            "intrinsic static method call to {}::{} with {} operands for {} params",
            receiver,
            method_name,
            inst.operands.len(),
            callee_sig.params.len()
        )));
    }
    let return_ty = callee_sig.return_type.clone();
    let callee_ref_params = callee_sig.ref_params.clone();
    let param_types = callee_sig
        .params
        .iter()
        .map(|(_, ty)| ty.codegen_repr())
        .collect::<Vec<_>>();
    let call_args = materialize_static_method_call_args_with_refs(
        ctx,
        called_class_id,
        &inst.operands,
        &param_types,
        &callee_ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(
        ctx.emitter,
        intrinsic
            .runtime_helper()
            .expect("runtime-backed static intrinsic must have a helper"),
    );
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    if let Some(result) = inst.result {
        let result_ty = ctx.value_php_type(result)?.codegen_repr();
        let return_ty = return_ty.codegen_repr();
        if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) && return_ty != PhpType::Mixed {
            emit_box_current_value_as_mixed(ctx.emitter, &return_ty);
        } else if return_ty == PhpType::Mixed
            && !matches!(result_ty, PhpType::Mixed | PhpType::Union(_))
        {
            cast_loaded_mixed_pointer_to_result(ctx, &result_ty)?;
        }
        ctx.store_result_value(result)?;
    }
    emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Lowers `CallbackFilterIterator::__elephcAcceptCallback()` through its stored descriptor.
pub(super) fn lower_callback_filter_accept_intrinsic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    intrinsic: IntrinsicCall,
) -> Result<()> {
    if inst.operands.len() != 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "{}::{} received {} operands for callback-filter accept",
            intrinsic.class_name(),
            intrinsic.method_key(),
            inst.operands.len()
        )));
    }
    let class_info = ctx
        .module
        .class_infos
        .get(intrinsic.class_name())
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "missing {} metadata for callback-filter accept",
                intrinsic.class_name()
            ))
        })?;
    let callback_offset = class_info
        .property_offsets
        .get("callback")
        .copied()
        .ok_or_else(|| CodegenIrError::missing_entry("property callback", 0))?;
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    ctx.load_value_to_reg(inst.operands[0], descriptor_reg)?;
    abi::emit_load_from_address(ctx.emitter, descriptor_reg, descriptor_reg, callback_offset);
    callables::emit_descriptor_reg_invoker_call_with_args(
        ctx,
        inst,
        descriptor_reg,
        &inst.operands[1..],
        "callback_filter_accept",
    )
}

