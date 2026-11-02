//! Purpose:
//! Lowers interface, nullable, and runtime-backed intrinsic method calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// One concrete implementation of a static method declared by an interface.
struct InterfaceStaticMethodCandidate {
    class_id: u64,
    class_name: String,
    impl_class: String,
    signature: FunctionSig,
}

/// Lowers an instance-method call through interface metadata.
pub(super) fn lower_interface_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    interface_name: &str,
    method_name: &str,
) -> Result<()> {
    lower_interface_method_call_with_miss(
        ctx,
        inst,
        interface_name,
        method_name,
        iterators::InterfaceDispatchMiss::ZeroResult,
    )
}

/// Lowers one interface method call with a chosen answer for a receiver no table matches.
pub(super) fn lower_interface_method_call_with_miss(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    interface_name: &str,
    method_name: &str,
    miss: iterators::InterfaceDispatchMiss,
) -> Result<()> {
    // Builtin Throwable methods are compact-payload intrinsics. Their interface vtable
    // slots stay zero because no synthetic method bodies are emitted, so dispatch here
    // would `blr` to null. Use the same intrinsic path as a concrete Throwable receiver.
    if is_throwable_standard_method_call(ctx, interface_name, method_name) {
        let object = expect_operand(inst, 0)?;
        return lower_throwable_standard_method(ctx, inst, object, method_name);
    }
    let normalized_interface = interface_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method_name);
    if ctx
        .module
        .interface_infos
        .get(normalized_interface)
        .is_some_and(|interface_info| interface_info.static_methods.contains_key(&method_key))
    {
        return lower_interface_static_method_call(
            ctx,
            inst,
            normalized_interface,
            method_name,
        );
    }
    if ctx
        .module
        .interface_infos
        .get(normalized_interface)
        .and_then(|interface_info| interface_info.methods.get(&method_key))
        .is_none()
    {
        return lower_narrowed_interface_method_call(
            ctx,
            inst,
            normalized_interface,
            method_name,
        );
    }
    let (normalized, method_key, callee_sig) =
        resolve_interface_call_signature(ctx, interface_name, method_name, inst.operands.len())?;
    let mut param_types = Vec::with_capacity(callee_sig.params.len() + 1);
    param_types.push(PhpType::Object(normalized.clone()));
    param_types.extend(callee_sig.params.iter().map(|(_, ty)| ty.codegen_repr()));
    let mut ref_params = Vec::with_capacity(callee_sig.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(callee_sig.ref_params.iter().copied());
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    ctx.load_value_to_reg(expect_operand(inst, 0)?, receiver_reg)?;
    if super::super::literal_method_index::has_literal_parameters(&callee_sig.params) {
        emit_interface_literal_parameter_guards(ctx, inst, method_name, receiver_reg)?;
    }
    let call_args = materialize_direct_call_args_with_refs_and_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
        crate::codegen::lower_inst::RefArgCellLifetime::for_callee(ctx, &method_name),
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    let return_ty = iterators::emit_interface_dispatch_call_with_miss(
        ctx,
        &normalized,
        &method_key,
        None,
        miss,
    )?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &return_ty)?;
    emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Dispatches an object-syntax call to a static interface method by concrete class id.
///
/// PHP permits `$object->staticMethod()`. For an interface-typed receiver the concrete class
/// selects the implementation and also supplies the hidden called-class id used by `static::`.
fn lower_interface_static_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    interface_name: &str,
    method_name: &str,
) -> Result<()> {
    let method_key = php_symbol_key(method_name);
    let interface_signature = ctx
        .module
        .interface_infos
        .get(interface_name)
        .and_then(|interface_info| interface_info.static_methods.get(&method_key))
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "interface static method call to unknown method {}::{}",
                interface_name, method_name
            ))
        })?
        .clone();
    // The receiver operand plus the source arguments; a family member reading its surplus
    // arguments gives the signature a hidden collector the call does not pass (see
    // `materialize_static_method_call_args_with_refs`).
    let omits_collector = inst.operands.len() == interface_signature.params.len()
        && crate::func_args::sig_collects_surplus_args(&interface_signature);
    if inst.operands.len() != interface_signature.params.len() + 1 && !omits_collector {
        return Err(CodegenIrError::unsupported(format!(
            "interface static method call to {}::{} with {} operands for {} ABI params",
            interface_name,
            method_name,
            inst.operands.len(),
            interface_signature.params.len() + 1
        )));
    }

    let mut candidates = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        if !class_implements_interface(ctx, class_name, interface_name) {
            continue;
        }
        let Some(signature) = class_info.static_methods.get(&method_key) else {
            continue;
        };
        let impl_class = class_info
            .static_method_impl_classes
            .get(&method_key)
            .cloned()
            .unwrap_or_else(|| class_name.clone());
        candidates.push(InterfaceStaticMethodCandidate {
            class_id: class_info.class_id,
            class_name: class_name.clone(),
            impl_class,
            signature: signature.clone(),
        });
    }
    candidates.sort_by_key(|candidate| candidate.class_id);

    let receiver = inst.operands[0];
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let no_match_label = ctx.next_label("interface_static_no_match");
    let done_label = ctx.next_label("interface_static_done");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "interface_static_{}",
                label_fragment(&candidate.class_name)
            ))
        })
        .collect::<Vec<_>>();
    ctx.load_value_to_reg(receiver, receiver_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [{}]", receiver_reg)); // load the concrete receiver class id for static interface dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare the runtime class id with this static implementation
                ctx.emitter.instruction(&format!("b.eq {}", label));            // branch to the matching concrete static method
            }
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r11, QWORD PTR [{}]", receiver_reg)); // load the concrete receiver class id for static interface dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare the runtime class id with this static implementation
                ctx.emitter.instruction(&format!("je {}", label));              // branch to the matching concrete static method
            }
        }
    }
    abi::emit_jump(ctx.emitter, &no_match_label);

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let param_types = candidate
            .signature
            .params
            .iter()
            .map(|(_, ty)| ty.codegen_repr())
            .collect::<Vec<_>>();
        let call_args = materialize_static_method_call_args_with_refs(
            ctx,
            &CalledClassIdArg::Immediate(candidate.class_id),
            &inst.operands[1..],
            &param_types,
            &candidate.signature.ref_params,
        )?;
        let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
        abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
        abi::emit_call_label(
            ctx.emitter,
            &static_method_symbol(&candidate.impl_class, &method_key),
        );
        abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
        abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
        store_call_result(ctx, inst, &candidate.signature.return_type)?;
        emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
        emit_ref_arg_writebacks(ctx, &call_args)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&no_match_label);
    exceptions::emit_error(
        ctx,
        &format!(
            "Call to undefined method {}::{}()",
            interface_name, method_name
        ),
    );
    ctx.emitter.label(&done_label);
    Ok(())
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
    // An internal dispatcher (`__toString` for a string conversion) passes the source arity; the
    // one missing parameter is the hidden collector `func_args::method_families` spreads over the
    // family, which the argument materializer stages empty.
    let omits_hidden_collector = operand_count + 1 == expected_args
        && crate::func_args::sig_collects_surplus_args(&callee_sig);
    if omits_hidden_collector {
        return Ok((normalized.to_string(), method_key, callee_sig));
    }
    let callee_sig = if operand_count == expected_args {
        callee_sig
    } else {
        common_interface_implementation_signature(ctx, normalized, &method_key, operand_count)
            .ok_or_else(|| {
                CodegenIrError::unsupported(format!(
                    "interface method call to {}::{} with {} operands for {} ABI params",
                    normalized, method_name, operand_count, expected_args
                ))
            })?
    };
    let expected_args = callee_sig.params.len() + 1;
    if operand_count != expected_args {
        return Err(CodegenIrError::unsupported(format!(
            "interface method call to {}::{} with {} operands for {} ABI params",
            normalized, method_name, operand_count, expected_args
        )));
    }
    Ok((normalized.to_string(), method_key, callee_sig))
}

/// Returns the shared concrete ABI used when an interface implementation adds optional parameters.
fn common_interface_implementation_signature(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    method_key: &str,
    operand_count: usize,
) -> Option<FunctionSig> {
    let mut signature = None;
    for (class_name, class_info) in &ctx.module.class_infos {
        if !class_implements_interface(ctx, class_name, interface_name) {
            continue;
        }
        let Some(candidate) = class_info.methods.get(method_key).cloned() else {
            continue;
        };
        if candidate.params.len() + 1 != operand_count {
            continue;
        }
        match &signature {
            Some(existing) if existing != &candidate => return None,
            Some(_) => {}
            None => signature = Some(candidate),
        }
    }
    signature
}

/// Lowers a method call after an earlier EIR guard has proven a nullable receiver non-null.
///
/// The null check stays ahead of argument checks. An overridable method then uses the
/// runtime class's declared parameters, so a wider override is not rejected by the base.
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
    let normalized_class = class_name.trim_start_matches('\\');
    if normalized_class.is_empty() {
        return lower_narrowed_nullable_interface_method_call(
            ctx, inst, object, normalized_class, method_name,
        );
    }
    if !ctx.module.class_infos.contains_key(normalized_class)
        && !ctx.module.extern_class_infos.contains_key(normalized_class)
        && !ctx.module.packed_class_infos.contains_key(normalized_class)
    {
        let null_label = ctx.next_label("unknown_method_receiver_null");
        let done_label = ctx.next_label("unknown_method_receiver_done");
        let receiver_reg = abi::nested_call_reg(ctx.emitter);
        objects::emit_nullable_receiver_object_payload(ctx, object, &null_label, receiver_reg)?;
        exceptions::emit_error(ctx, &format!("Class \"{}\" not found", normalized_class));
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&null_label);
        emit_method_call_on_null_fatal(ctx, method_name);
        ctx.emitter.label(&done_label);
        return Ok(());
    }
    // A built-in Throwable class has no method bodies behind its vtable slots: the typed path
    // answers `getMessage()` and the rest from the compact payload, and so must the nullable one.
    // `function g(): ?LogicException` then `g()->getMessage()` jumped to a null slot.
    if is_throwable_standard_method_call(ctx, class_name, method_name) {
        return lower_nullable_throwable_standard_method(ctx, inst, object, method_name);
    }
    if !class_declares_method(ctx, class_name, method_name)
        && !narrowed_interface_candidates(ctx, class_name, method_name, inst.operands.len())?
            .is_empty()
    {
        return lower_narrowed_nullable_interface_method_call(
            ctx,
            inst,
            object,
            class_name.trim_start_matches('\\'),
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
    if super::method_resolution::method_requires_runtime_literal_guards(ctx, class_name, &target) {
        emit_hierarchy_literal_parameter_guards(
            ctx, inst, class_name, method_name, receiver_reg,
        )?;
    } else {
        abi::emit_push_reg(ctx.emitter, receiver_reg);
        super::method_resolution::emit_method_literal_parameter_guards(
            ctx, &target, &inst.operands[1..],
        )?;
        abi::emit_pop_reg(ctx.emitter, receiver_reg);
    }
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
        crate::codegen::lower_inst::RefArgCellLifetime::for_callee(ctx, &method_name),
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    emit_resolved_method_call(ctx, &target)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_method_call_result(ctx, inst, &target)?;
    emit_call_arg_temp_cleanups(ctx, &call_args, None)?;
    emit_ref_arg_writebacks(ctx, &call_args)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_method_call_on_null_fatal(ctx, method_name);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a compact-payload Throwable method on a nullable receiver, fataling on null.
fn lower_nullable_throwable_standard_method(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    let null_label = ctx.next_label("method_receiver_null");
    let done_label = ctx.next_label("method_receiver_done");
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    objects::emit_nullable_receiver_object_payload(ctx, object, &null_label, receiver_reg)?;
    // The unboxed object is already in `receiver_reg`; the shared lowerer must not reload the
    // SSA operand, which may still be the boxed cell.
    lower_throwable_standard_method_from_reg(ctx, inst, receiver_reg, method_name)?;
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
        return lower_nullable_throwable_standard_method(ctx, inst, object, method_name);
    }
    let normalized = interface_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method_name);
    if ctx
        .module
        .interface_infos
        .get(normalized)
        .and_then(|interface_info| interface_info.methods.get(&method_key))
        .is_none()
    {
        return lower_narrowed_nullable_interface_method_call(
            ctx,
            inst,
            object,
            normalized,
            method_name,
        );
    }
    // A site lowered against the implementations' shared signature passes their defaulted extra
    // parameters too; the non-null receiver takes the same ABI a typed receiver does.
    let (_, _, callee_sig) =
        resolve_interface_call_signature(ctx, normalized, method_name, inst.operands.len())?;
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
    if super::super::literal_method_index::has_literal_parameters(&callee_sig.params) {
        emit_interface_literal_parameter_guards(ctx, inst, method_name, receiver_reg)?;
    }
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
        crate::codegen::lower_inst::RefArgCellLifetime::for_callee(ctx, &method_name),
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    let return_ty = iterators::emit_interface_dispatch_call(ctx, normalized, &method_key, None)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &return_ty)?;
    emit_call_arg_temp_cleanups(ctx, &call_args, None)?;
    emit_ref_arg_writebacks(ctx, &call_args)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_method_call_on_null_fatal(ctx, method_name);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits PHP's fatal diagnostic for calling an instance method on null.
pub(super) fn emit_method_call_on_null_fatal(ctx: &mut FunctionContext<'_>, method_name: &str) {
    emit_method_call_on_null_error(ctx, method_name, true);
}

/// The null-method-call error, with the bridge trace optional so a caller that already traced
/// (through the shared helper's combined raw-and-plain form) does not trace twice.
pub(super) fn emit_method_call_on_null_error(
    ctx: &mut FunctionContext<'_>,
    method_name: &str,
    trace: bool,
) {
    if trace
        && !crate::codegen::shared_null_method_trace::emit_shared_null_method_trace(
            ctx,
            method_name,
            None,
        )
        .unwrap_or(false)
    {
        emit_aot_null_method_receiver_trace(ctx, method_name);
    }
    exceptions::emit_error(
        ctx,
        &format!("Call to a member function {}() on null", method_name),
    );
}

/// Emits the opt-in eval-bridge trace for an AOT null method receiver.
fn emit_aot_null_method_receiver_trace(ctx: &mut FunctionContext<'_>, method_name: &str) {
    if !ctx.module.required_runtime_features.eval_bridge {
        return;
    }
    let (function_label, function_len) = ctx.data.add_string(ctx.function.name.as_bytes());
    let (method_label, method_len) = ctx.data.add_string(method_name.as_bytes());
    let function_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_symbol_address(ctx.emitter, function_ptr_arg, &function_label);
    let function_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_int_immediate(ctx.emitter, function_len_arg, function_len as i64);
    let method_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, method_ptr_arg, &method_label);
    let method_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_int_immediate(ctx.emitter, method_len_arg, method_len as i64);
    let line_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    let line = ctx
        .current_instruction_span()
        .map(|span| span.source_line() as i64)
        .unwrap_or_default();
    abi::emit_load_int_immediate(ctx.emitter, line_arg, line);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_trace_aot_null_method_receiver");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    // A writer that adopts its value box needs a reference of its own when the operand already is
    // a box: that box belongs to the caller, which releases it after the call. A concrete operand
    // is boxed afresh by the materialization below and that box is handed over as it is. Without
    // this, `$stack->push(7)` stored a box the caller then freed, and the next allocation of that
    // block rewrote the element (`pop()` answered the stack itself, or heap debug reported
    // free-list corruption).
    if let Some(index) = intrinsic.kind().adopted_value_operand() {
        if let Some(&value) = inst.operands.get(index) {
            if matches!(
                ctx.value_php_type(value)?.codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            ) {
                ctx.load_value_to_result(value)?;
                abi::emit_call_label(ctx.emitter, "__rt_incref"); // the runtime keeps this box; the caller keeps its own
            }
        }
    }
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

/// Checks the concrete implementation before interface argument coercion.
/// Wider contravariant implementations retain their own accepted parameter types.
fn emit_interface_literal_parameter_guards(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    method_name: &str,
    receiver_reg: &str,
) -> Result<()> {
    emit_dispatched_literal_parameter_guards(ctx, inst, method_name, receiver_reg, None)
}

/// Checks literal parameters on the runtime class when a base method can be overridden.
///
/// `false` and `null` on the static signature are not the implementation's contract:
/// a subclass may widen them. Normally only classes in `ancestor`'s tree are considered.
/// Final generic declarations also consider variance siblings without inheritance edges,
/// selecting the actual declaring instantiation's diagnostic name by runtime class id.
pub(super) fn emit_hierarchy_literal_parameter_guards(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    ancestor: &str,
    method_name: &str,
    receiver_reg: &str,
) -> Result<()> {
    let key = php_symbol_key(method_name);
    let normalized = ancestor.trim_start_matches('\\');
    if !ctx.module.class_infos.get(normalized)
        .and_then(|class| class.methods.get(&key))
        .is_some_and(|signature| super::super::literal_method_index::has_literal_parameters(&signature.params))
    {
        return Ok(());
    }
    let fixed_generic_signature = normalized.contains('<')
        && ctx.module.class_infos.get(normalized).is_some_and(|class| {
            class.is_final || class.final_methods.contains(&key)
        });
    emit_dispatched_literal_parameter_guards(
        ctx, inst, method_name, receiver_reg, if fixed_generic_signature { None } else { Some(ancestor) },
    )
}

/// Dispatches literal parameter checks by runtime class id.
///
/// `ancestor` limits the search to one class and its subclasses. Interface calls pass
/// `None` so every implementation of the method is eligible.
fn emit_dispatched_literal_parameter_guards(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    method_name: &str,
    receiver_reg: &str,
    ancestor: Option<&str>,
) -> Result<()> {
    let indexed = ctx.shared.literal_methods.candidates(
        ctx.module, &php_symbol_key(method_name), false, inst.operands.len() - 1,
    );
    let candidates = indexed.iter().filter(|candidate| ancestor.is_none_or(|ancestor|
        super::method_dispatch::class_is_same_or_subclass(ctx, &candidate.class_name, ancestor)
    )).collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(());
    }
    let done = ctx.next_label("interface_literal_done");
    let labels = candidates.iter().map(|_| ctx.next_label("interface_literal_class"))
        .collect::<Vec<_>>();
    // -- select the declared parameter contract from the receiver's runtime class --
    abi::emit_push_reg(ctx.emitter, receiver_reg);
    let class_id_reg = abi::symbol_scratch_reg(ctx.emitter);
    let compare_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, class_id_reg, receiver_reg, 0);
    for (candidate, label) in candidates.iter().zip(&labels) {
        abi::emit_load_int_immediate(ctx.emitter, compare_reg, candidate.class_id as i64);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, compare_reg)); // compare the runtime receiver with this literal contract
                ctx.emitter.instruction(&format!("b.eq {}", label));            // validate only the selected runtime implementation
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, compare_reg)); // compare the runtime receiver with this literal contract
                ctx.emitter.instruction(&format!("je {}", label));              // validate only the selected runtime implementation
            }
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    for (candidate, label) in candidates.iter().zip(&labels) {
        ctx.emitter.label(label);
        super::direct_calls::emit_literal_declared_parameter_guards(
            ctx, &inst.operands[1..], &candidate.params, &candidate.callable_name,
        )?;
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&done);
    abi::emit_pop_reg(ctx.emitter, receiver_reg);
    Ok(())
}
