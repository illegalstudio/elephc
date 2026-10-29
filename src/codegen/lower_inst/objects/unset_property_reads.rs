//! Purpose:
//! Fetches an unset property's container through runtime class and name metadata.
//!
//! Called from:
//! - DynamicPropGet with the QuietRead mode used by opaque property dimension unset.
//!
//! Key details:
//! - Declared slots stay quiet when uninitialized; magic getters bypass isset guards.
//! - Missing dynamic slots become null through their actual owning hash storage.
//! - All argument registers and stack operations use the shared target ABI helpers.

use super::*;

/// Dispatches the captured receiver/name without compile-time-only member query builtins.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, object: ValueId, name: ValueId,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, name, inst)?;
    ensure_dynamic_property_miss_supported(inst)?;
    let candidates = declared_mixed_property_get_candidates(ctx, PropertyFetchMode::QuietRead, inst)?;
    let mut classes = ctx.module.class_infos.iter()
        .map(|(name, info)| (name.clone(), info.class_id)).collect::<Vec<_>>();
    classes.sort_by_key(|(_, id)| *id);
    let labels = classes.iter().map(|_| ctx.next_label("unset_prop_class")).collect::<Vec<_>>();
    let slot_labels = candidates.iter().map(|_| ctx.next_label("unset_prop_slot")).collect::<Vec<_>>();
    let scalar = ctx.next_label("unset_prop_scalar");
    let done = ctx.next_label("unset_prop_done");
    let result = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, result)?;
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        emit_branch_if_mixed_unboxed_not_object(ctx, &scalar);
        push_mixed_unboxed_object_payload(ctx);
    } else { abi::emit_push_reg(ctx.emitter, result); }
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(name, ptr, len)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr, len);
    for (candidate, label) in candidates.iter().zip(&slot_labels) {
        emit_branch_if_mixed_dynamic_property_candidate_matches(
            ctx, candidate.candidate.class_id, &candidate.candidate.slot.property, label,
        );
    }
    for ((_, id), label) in classes.iter().zip(&labels) {
        emit_branch_if_stacked_object_class_matches(ctx, *id, 16, label);
    }
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    super::super::exceptions::emit_error(ctx, "Unsupported unset(): opaque property has no runtime storage metadata");
    abi::emit_jump(ctx.emitter, &done);

    for (candidate, label) in candidates.iter().zip(&slot_labels) {
        ctx.emitter.label(label);
        let slot = &candidate.candidate.slot;
        match &candidate.kind {
            MixedPropertyReadKind::Refuse(message) => {
                let receiver = abi::symbol_scratch_reg(ctx.emitter);
                abi::emit_load_temporary_stack_slot(ctx.emitter, receiver, 16);
                abi::emit_release_temporary_stack(ctx.emitter, 32);
                emit_refused_read_or_eval_child_property(
                    ctx, slot, message, receiver, PropertyFetchMode::QuietRead, &done,
                )?;
            }
            MixedPropertyReadKind::Slot => {
                let receiver = abi::symbol_scratch_reg(ctx.emitter);
                abi::emit_load_temporary_stack_slot(ctx.emitter, receiver, 16);
                let missing = emit_property_read_state_guard(ctx, slot, receiver,
                    PropertyFetchMode::QuietRead, PropertyReadMissingResult::Instruction(inst))?;
                emit_property_load(ctx, slot, receiver)?;
                materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
                if let Some(missing) = missing { ctx.emitter.label(&missing); }
                abi::emit_release_temporary_stack(ctx.emitter, 32);
            }
            MixedPropertyReadKind::ScopeDynamic | MixedPropertyReadKind::MagicDeferred => {
                let position = classes.iter().position(|(_, id)| *id == candidate.candidate.class_id)
                    .ok_or_else(|| CodegenIrError::invalid_module("unset property candidate has no runtime class"))?;
                abi::emit_jump(ctx.emitter, &labels[position]);
                continue;
            }
        }
        abi::emit_jump(ctx.emitter, &done);
    }
    for ((class, _), label) in classes.iter().zip(&labels) {
        ctx.emitter.label(label);
        read_dynamic(ctx, inst, object, name, class)?;
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&scalar);
    emit_dynamic_property_miss_result(ctx, inst);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Checks dynamic storage before invoking a getter or creating a missing null field.
fn read_dynamic(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, object: ValueId, name: ValueId, class: &str,
) -> Result<()> {
    let hash = dynamic_property_hash_offset_for_class(ctx, class, "")?;
    let found = ctx.next_label("unset_prop_hash_found");
    let done = ctx.next_label("unset_prop_hash_done");
    if let Some(hash) = hash {
        load_hash_arguments(ctx, hash);
        abi::emit_call_label(ctx.emitter, "__rt_hash_get");
        emit_branch_if_hash_entry_found(ctx, &found);
    }
    if class_declares_method(ctx, class, "__get") {
        emit_getter(ctx, inst, object, name, class)?;
    } else if class_declares_method(ctx, class, "__set") || hash.is_none() {
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        super::super::exceptions::emit_error(ctx,
            "Unsupported unset(): missing opaque property requires a raw storage fetch");
    } else {
        let hash = hash.expect("dynamic storage checked above");
        emit_dynamic_property_creation_deprecation(ctx, class, hash, 16, 0)?;
        create_null(ctx, hash);
    }
    abi::emit_jump(ctx.emitter, &done);
    if hash.is_some() {
        ctx.emitter.label(&found);
        let target = ctx.emitter.target;
        let value = match target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rdi" };
        abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
        abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Loads hash and runtime name while the receiver/name block remains available on a miss.
fn load_hash_arguments(ctx: &mut FunctionContext<'_>, hash: usize) {
    let target = ctx.emitter.target;
    let receiver = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, receiver, 16);
    abi::emit_load_from_address(ctx.emitter, abi::int_arg_reg_name(target, 0), receiver, hash);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 2), 8);
}

/// Invokes a runtime-name getter through the normal method ABI after releasing dispatch scratch.
fn emit_getter(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, object: ValueId, name: ValueId, class: &str,
) -> Result<()> {
    let target = resolve_method_call_target(ctx, class, "__get", 2)?;
    let receiver = abi::nested_call_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &receiver, 16);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    let receiver_ty = PhpType::Object(class.into());
    let mut params = vec![receiver_ty.clone()];
    params.extend(target.params.iter().map(PhpType::codegen_repr));
    let mut refs = vec![false];
    refs.extend(target.ref_params.iter().copied());
    let args = materialize_method_call_args_with_receiver_reg_and_refs(ctx, &receiver, &receiver_ty,
        &[object, name], &params, &refs, RefArgCellLifetime::CallOnly)?;
    let pad = direct_call_stack_pad_bytes(ctx, args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, pad);
    emit_resolved_method_call(ctx, &target)?;
    abi::emit_release_temporary_stack(ctx.emitter, pad);
    abi::emit_release_temporary_stack(ctx.emitter, args.overflow_bytes);
    if target.by_ref_return {
        copy_reference_result(ctx, inst, &target.return_ty)?;
    } else { store_method_call_result(ctx, inst, &target)?; }
    emit_call_arg_temp_cleanups(ctx, &args, None)?;
    emit_ref_arg_writebacks(ctx, &args)?;
    let result = inst.result.ok_or_else(|| CodegenIrError::invalid_module("unset property read has no result"))?;
    ctx.load_value_to_reg(result, abi::int_result_reg(ctx.emitter))?;
    Ok(())
}

/// Copies a getter's returned cell before retiring its lease or scalar-name argument owners.
/// No user code runs between the transferred lease and its acquired value snapshot.
fn copy_reference_result(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, ty: &PhpType,
) -> Result<()> {
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let cell = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, cell, 0);
    let repr = ty.codegen_repr();
    match &repr {
        PhpType::Str => {
            let (ptr, len) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, ptr, cell, 0);
            abi::emit_load_from_address(ctx.emitter, len, cell, 8);
        }
        PhpType::Float => abi::emit_load_from_address(ctx.emitter, abi::float_result_reg(ctx.emitter), cell, 0),
        PhpType::TaggedScalar => {
            abi::emit_load_from_address(ctx.emitter, abi::int_result_reg(ctx.emitter), cell, 0);
            abi::emit_load_from_address(ctx.emitter, crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter), cell, 8);
        }
        _ => abi::emit_load_from_address(ctx.emitter, abi::int_result_reg(ctx.emitter), cell, 0),
    }
    if matches!(repr, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    } else { emit_box_current_value_as_mixed(ctx.emitter, &repr); }
    store_if_result(ctx, inst)?;
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    // The snapshot retains the payload, so dropping the lease cannot destroy that payload.
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_release");
    Ok(())
}

/// Inserts a null cell through the owning hash slot, preserving lazy allocation and COW.
fn create_null(ctx: &mut FunctionContext<'_>, hash: usize) {
    emit_boxed_null(ctx);
    // Hash insertion consumes one owner; the returned container read keeps the other.
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let target = ctx.emitter.target;
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 0), 32);
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch, hash as i64);
    match target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("add x0, x0, {scratch}")), // locate the receiver's owning dynamic-property hash slot
        Arch::X86_64 => ctx.emitter.instruction(&format!("add rdi, {scratch}")), // locate the receiver's owning dynamic-property hash slot
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), 16);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 2), 24);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 3), 0);
    abi::emit_call_label(ctx.emitter, "__rt_property_hash_set");
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_release_temporary_stack(ctx.emitter, 32);
}
