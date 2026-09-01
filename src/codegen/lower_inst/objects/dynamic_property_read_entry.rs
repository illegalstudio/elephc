//! Purpose:
//! Lowers nullsafe and runtime-name property-read entry points.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Name evaluation and null-container warnings remain PHP-observable in the same order.

use super::*;

/// Lowers a nullsafe declared-property read for nullable object receivers.
pub(in crate::codegen::lower_inst) fn lower_nullsafe_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    let Some((class_name, nullable)) = nullable_object_receiver_class(ctx, object)? else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for receiver PHP type {:?}",
            inst.op.name(),
            raw_value_php_type(ctx, object)?
        )));
    };
    if !nullable {
        return lower_prop_get(ctx, inst);
    }
    let slot = resolve_property_slot_for_class(ctx, &class_name, &property, inst)?;
    let null_label = ctx.next_label("nullsafe_prop_null");
    let done_label = ctx.next_label("nullsafe_prop_done");
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    emit_nullable_receiver_object_payload(ctx, object, &null_label, base_reg)?;
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    emit_property_load(ctx, &slot, base_reg)?;
    materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    emit_boxed_null(ctx);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers a dynamic property read against declared slots on statically known objects.
pub(in crate::codegen::lower_inst) fn lower_dynamic_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property_value = expect_operand(inst, 1)?;
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Object(_)) {
        return lower_object_dynamic_prop_get_with_null_guard(
            ctx,
            inst,
            object,
            property_value,
        );
    }
    lower_dynamic_prop_get_nonnull(ctx, inst, object, property_value)
}

/// Guards a statically typed object before evaluating any runtime-name property
/// representation that would otherwise dereference the null-container sentinel.
pub(super) fn lower_object_dynamic_prop_get_with_null_guard(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property_value: ValueId,
) -> Result<()> {
    let null_label = ctx.next_label("dynamic_prop_get_null_receiver");
    let done_label = ctx.next_label("dynamic_prop_get_done");
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        object_reg,
        scratch_reg,
        &null_label,
    );
    lower_dynamic_prop_get_nonnull(ctx, inst, object, property_value)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_dynamic_property_on_null_warning(ctx, property_value)?;
    // Property reads keep the legacy zero-float miss shape: their null result is never
    // re-tested for null the way a silent `??` element read is.
    super::super::arrays::emit_array_get_null_fallback(ctx, &inst.result_php_type.codegen_repr(), false);
    store_if_result(ctx, inst)?;

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Selects dynamic-property lowering after a typed object receiver has been
/// proven non-null, or for receiver shapes with their own runtime null checks.
pub(super) fn lower_dynamic_prop_get_nonnull(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property_value: ValueId,
) -> Result<()> {
    if let Some(property) = const_string_operand(ctx, property_value)? {
        return lower_const_dynamic_prop_get(ctx, object, property, inst);
    }
    if matches!(
        ctx.value_php_type(object)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return lower_runtime_dynamic_mixed_prop_get(ctx, inst, object, property_value);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_runtime_dynamic_stdclass_prop_get(ctx, inst, object, property_value);
    }
    lower_runtime_dynamic_declared_prop_get(ctx, object, property_value, inst)
}

/// Emits PHP's runtime-name warning for a dynamic property read on null.
pub(super) fn emit_dynamic_property_on_null_warning(
    ctx: &mut FunctionContext<'_>,
    property_value: ValueId,
) -> Result<()> {
    emit_property_warning_fragment(ctx, b"Warning: Attempt to read property \"");
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.load_string_value_to_regs(property_value, "x1", "x2")?,
        Arch::X86_64 => ctx.load_string_value_to_regs(property_value, "rdi", "rsi")?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    emit_property_warning_fragment(ctx, b"\" on null\n");
    Ok(())
}

/// Writes one static fragment through the suppressible PHP warning channel.
pub(super) fn emit_property_warning_fragment(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}

/// Lowers a dynamic property read when the property expression is a literal string.
pub(super) fn lower_const_dynamic_prop_get(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<()> {
    if matches!(
        ctx.value_php_type(object)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return lower_mixed_prop_get(ctx, inst, object, property);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_stdclass_prop_get(ctx, inst, object, property);
    }
    if let Some(class_name) = magic_get_receiver_class(ctx, object, property)? {
        return lower_magic_get_prop(ctx, inst, object, &class_name, property);
    }
    if let Some(offset) = dynamic_property_hash_offset_for_object(ctx, object, property)? {
        return lower_allow_dynamic_prop_get(ctx, inst, object, property, offset);
    }
    let slot = resolve_property_slot(ctx, object, property, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    emit_property_load(ctx, &slot, base_reg)?;
    materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
    store_if_result(ctx, inst)
}

/// Lowers a runtime-name dynamic property read from a boxed `Mixed` receiver.
pub(super) fn lower_runtime_dynamic_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property_value: ValueId,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    ensure_dynamic_property_miss_supported(inst)?;
    let candidates = declared_mixed_property_get_candidates(ctx, inst)?;
    let done_label = ctx.next_label("mixed_dyn_prop_get_done");
    let miss_label = ctx.next_label("mixed_dyn_prop_get_miss");
    let miss_no_stack_label = ctx.next_label("mixed_dyn_prop_get_miss_no_stack");
    let stdclass_label = ctx.next_label("mixed_dyn_prop_get_stdclass");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_dyn_prop_get_{}",
                label_fragment(&candidate.slot.property)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_not_object(ctx, &miss_no_stack_label);
    push_mixed_unboxed_object_payload(ctx);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        emit_branch_if_mixed_dynamic_property_candidate_matches(ctx, candidate, label);
    }
    emit_branch_if_stacked_object_is_stdclass(ctx, 16, &stdclass_label);
    abi::emit_jump(ctx.emitter, &miss_label);

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, 16);
        if let Some(target) =
            property_hook_get_target(ctx, &candidate.slot.class_name, &candidate.slot.property)?
        {
            emit_property_hook_get_result(ctx, inst, object, base_reg, &candidate.slot, &target)?;
        } else {
            if candidate.slot.is_declared {
                emit_uninitialized_typed_property_guard(ctx, &candidate.slot, base_reg);
            }
            emit_property_load(ctx, &candidate.slot, base_reg)?;
            materialize_loaded_property_result(ctx, inst, &candidate.slot.php_type)?;
        }
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&stdclass_label);
    emit_runtime_stdclass_get_for_stacked_name(ctx, inst, 16, 0)?;
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    emit_dynamic_property_miss_result(ctx, inst);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_no_stack_label);
    emit_dynamic_property_miss_result(ctx, inst);

    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}
