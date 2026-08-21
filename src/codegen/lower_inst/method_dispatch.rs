//! Purpose:
//! Lowers direct and boxed-Mixed instance method dispatch.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers a direct instance-method call on a statically known object receiver.
pub(super) fn lower_method_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let method_name = method_name_data(ctx, inst)?.to_string();
    if let Some((class_name, true)) = objects::nullable_object_receiver_class(ctx, object)? {
        return lower_nullable_receiver_method_call(ctx, inst, object, &class_name, &method_name);
    }
    let object_ty = ctx.value_php_type(object)?.codegen_repr();
    if matches!(object_ty, PhpType::Mixed | PhpType::Union(_)) {
        if let Some(state) = fiber_state_predicate_method(&method_name) {
            return lower_mixed_fiber_state_predicate(ctx, inst, object, &method_name, state);
        }
        return lower_mixed_method_call(ctx, inst, object, &method_name);
    }
    let PhpType::Object(class_name) = object_ty else {
        return Err(CodegenIrError::unsupported(format!(
            "method call receiver for PHP type {:?}",
            object_ty
        )));
    };
    guard_static_method_receiver(ctx, object, &method_name)?;
    if let Some(state) = fiber_state_predicate(&class_name, &method_name) {
        return lower_fiber_state_predicate(ctx, inst, object, state);
    }
    if let Some(intrinsic) = generator_intrinsic(&class_name, &method_name) {
        return lower_generator_intrinsic(ctx, inst, intrinsic);
    }
    if let Some(intrinsic) = callback_filter_intrinsic(&class_name, &method_name) {
        return lower_callback_filter_accept_intrinsic(ctx, inst, intrinsic);
    }
    if is_fiber_start_call(&class_name, &method_name) {
        return lower_fiber_start(ctx, inst, object);
    }
    if is_fiber_resume_call(&class_name, &method_name) {
        return lower_fiber_resume(ctx, inst, object);
    }
    if is_fiber_throw_call(&class_name, &method_name) {
        return lower_fiber_throw(ctx, inst, object);
    }
    if is_fiber_get_return_call(&class_name, &method_name) {
        return lower_fiber_noarg_runtime_method(ctx, inst, object, "__rt_fiber_get_return");
    }
    if let Some(intrinsic) = runtime_backed_instance_intrinsic(&class_name, &method_name) {
        return lower_instance_runtime_intrinsic(ctx, inst, &class_name, &method_name, intrinsic);
    }
    if is_throwable_standard_method_call(ctx, &class_name, &method_name) {
        return lower_throwable_standard_method(ctx, inst, object, &method_name);
    }
    if ctx
        .module
        .interface_infos
        .contains_key(class_name.trim_start_matches('\\'))
    {
        return lower_interface_method_call(ctx, inst, &class_name, &method_name);
    }
    let target = resolve_method_call_target(ctx, &class_name, &method_name, inst.operands.len())?;
    let mut param_types = Vec::with_capacity(target.params.len() + 1);
    param_types.push(PhpType::Object(class_name));
    param_types.extend(target.params.iter().map(|param| param.codegen_repr()));
    let mut ref_params = Vec::with_capacity(target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(target.ref_params.iter().copied());
    let call_args = materialize_direct_call_args_with_refs_and_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
        crate::codegen::lower_inst::RefArgCellLifetime::CallOnly,
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
    emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Rejects the raw null-container representation before a static object method dispatch.
pub(super) fn guard_static_method_receiver(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    let receiver_reg = abi::symbol_scratch_reg(ctx.emitter);
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    let null_label = ctx.next_label("static_method_receiver_null");
    let done_label = ctx.next_label("static_method_receiver_checked");
    ctx.load_value_to_reg(object, receiver_reg)?;
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        receiver_reg,
        scratch_reg,
        &null_label,
    );
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    emit_method_call_on_null_fatal(ctx, method_name);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers an instance-method call whose receiver is boxed as `Mixed`.
pub(super) fn lower_mixed_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    let candidates = mixed_method_candidates(ctx, method_name, inst.operands.len())?;
    if candidates.is_empty() {
        if builtins::has_eval_context(ctx) {
            return builtins::lower_eval_method_call(ctx, inst, object, method_name);
        }
        emit_method_call_on_null_fatal(ctx, method_name);
        return Ok(());
    }

    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let non_object_label = ctx.next_label("mixed_method_non_object");
    let no_match_label = ctx.next_label("mixed_method_no_match");
    let done_label = ctx.next_label("mixed_method_done");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_method_{}",
                label_fragment(&candidate.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_result(object)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_mixed_method_object_payload_or_fatal(ctx, receiver_reg, &non_object_label);
    emit_mixed_method_class_dispatch(
        ctx,
        receiver_reg,
        &candidates,
        &match_labels,
        &no_match_label,
    );

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        lower_mixed_method_candidate_call(ctx, inst, receiver_reg, candidate, method_name)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&no_match_label);
    if builtins::has_eval_context(ctx) {
        builtins::lower_eval_method_call(ctx, inst, object, method_name)?;
        abi::emit_jump(ctx.emitter, &done_label);
    } else {
        emit_method_call_on_null_fatal(ctx, method_name);
    }

    ctx.emitter.label(&non_object_label);
    emit_method_call_on_null_fatal(ctx, method_name);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits one concrete class branch for a `Mixed` receiver method call.
pub(super) fn lower_mixed_method_candidate_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver_reg: &str,
    candidate: &MixedMethodCandidate,
    method_name: &str,
) -> Result<()> {
    // Built-in Throwables implement the standard Throwable surface through compact intrinsics,
    // not through class vtable slots — those slots stay empty for builtins. Dispatching this
    // candidate dynamically would load a null slot and branch to it, so route it to the same
    // intrinsic the direct-receiver path uses. The receiver payload is already unboxed in
    // `receiver_reg`, which is exactly what `_from_reg` expects.
    if is_throwable_standard_method_call(ctx, &candidate.class_name, method_name) {
        return lower_throwable_standard_method_from_reg(ctx, inst, receiver_reg, method_name);
    }
    let receiver_ty = PhpType::Object(candidate.class_name.clone());
    let mut param_types = Vec::with_capacity(candidate.target.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(
        candidate
            .target
            .params
            .iter()
            .map(|param| param.codegen_repr()),
    );
    let mut ref_params = Vec::with_capacity(candidate.target.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(candidate.target.ref_params.iter().copied());
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &inst.operands,
        &param_types,
        &ref_params,
        crate::codegen::lower_inst::RefArgCellLifetime::CallOnly,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    if let Some(slot) = candidate.target.dynamic_slot {
        emit_dynamic_instance_method_call(ctx, slot);
    } else {
        abi::emit_call_label(
            ctx.emitter,
            &method_symbol(&candidate.target.impl_class, &candidate.target.method_key),
        );
    }
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_method_call_result(ctx, inst, &candidate.target)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Collects concrete class-method candidates for a boxed `Mixed` receiver.
pub(super) fn mixed_method_candidates(
    ctx: &FunctionContext<'_>,
    method_name: &str,
    operand_count: usize,
) -> Result<Vec<MixedMethodCandidate>> {
    let method_key = php_symbol_key(method_name);
    let mut candidates = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        let Some(signature) = class_info.methods.get(&method_key) else {
            continue;
        };
        if signature.params.len() + 1 != operand_count {
            continue;
        }
        let target = resolve_method_call_target(ctx, class_name, method_name, operand_count)?;
        candidates.push(MixedMethodCandidate {
            class_id: class_info.class_id,
            class_name: class_name.clone(),
            target,
        });
    }
    candidates.sort_by_key(|candidate| candidate.class_id);
    Ok(candidates)
}

/// Preserves the unboxed object payload or routes non-object `Mixed` receivers to fatal.
pub(super) fn emit_mixed_method_object_payload_or_fatal(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    no_match_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // require an object payload before method dispatch
            ctx.emitter.instruction(&format!("b.ne {}", no_match_label));       // non-object Mixed receivers cannot call instance methods
            ctx.emitter
                .instruction(&format!("mov {}, x1", receiver_reg)); // preserve the unboxed object payload across argument lowering
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // require an object payload before method dispatch
            ctx.emitter.instruction(&format!("jne {}", no_match_label));        // non-object Mixed receivers cannot call instance methods
            ctx.emitter
                .instruction(&format!("mov {}, rdi", receiver_reg)); // preserve the unboxed object payload across argument lowering
        }
    }
}

/// Emits class-id branches for every method candidate discovered for a `Mixed` receiver.
pub(super) fn emit_mixed_method_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    candidates: &[MixedMethodCandidate],
    match_labels: &[String],
    no_match_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [{}]", receiver_reg)); // load the receiver class id for Mixed method dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare the receiver class id against this method candidate
                ctx.emitter.instruction(&format!("b.eq {}", label));            // call this candidate when the runtime class id matches
            }
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r11, QWORD PTR [{}]", receiver_reg)); // load the receiver class id for Mixed method dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare the receiver class id against this method candidate
                ctx.emitter.instruction(&format!("je {}", label));              // call this candidate when the runtime class id matches
            }
        }
    }
    abi::emit_jump(ctx.emitter, no_match_label);
}

/// Re-exports the shared label fragmenter so instruction lowering keeps one implementation.
///
/// `crate::names::label_fragment` is documented as deliberately NON-injective — every
/// non-alphanumeric byte collapses to `_`, so `a_b` and `aéb` collide. A second copy here
/// invited use where uniqueness matters; there is now one definition carrying that warning.
pub(super) use crate::names::label_fragment;

