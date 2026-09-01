//! Purpose:
//! Lowers static and runtime-name property writes across receiver shapes.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Declared-slot dispatch and Mixed object validation preserve value ownership.

use super::*;

/// Lowers a declared object property write for statically known object receivers.
pub(in crate::codegen::lower_inst) fn lower_prop_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    if let Some((class_name, true)) = nullable_object_receiver_class(ctx, object)? {
        return lower_nullable_prop_set(ctx, inst, object, value, &class_name, &property);
    }
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Mixed) {
        return lower_mixed_prop_set(ctx, object, value, &property);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_stdclass_prop_set(ctx, object, value, &property);
    }
    if let Some(offset) = dynamic_property_hash_offset_for_object(ctx, object, &property)? {
        return lower_allow_dynamic_prop_set(ctx, object, value, &property, offset, inst.span);
    }
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    if is_promoted_reference_property_bind(ctx, object, value, &slot)? {
        return emit_reference_property_bind(ctx, value, &slot, base_reg);
    }
    emit_property_store(ctx, value, &slot, base_reg)
}

/// Lowers a dynamic property write (`$object->{$name} = $value`).
pub(in crate::codegen::lower_inst) fn lower_dynamic_prop_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property_value = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    if let Some(property) = const_string_operand(ctx, property_value)? {
        return lower_const_dynamic_prop_set(ctx, object, value, property, inst);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_runtime_stdclass_prop_set(ctx, object, property_value, value, inst);
    }
    match ctx.value_php_type(object)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            lower_runtime_mixed_prop_set(ctx, object, property_value, value, inst)
        }
        PhpType::Object(class_name) => {
            lower_runtime_object_prop_set(ctx, object, property_value, value, &class_name, inst)
        }
        object_ty => Err(CodegenIrError::unsupported(format!(
            "{} for receiver PHP type {:?}",
            inst.op.name(),
            object_ty
        ))),
    }
}

/// Lowers a dynamic property write when the name expression folded to a string.
pub(super) fn lower_const_dynamic_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<()> {
    if matches!(
        ctx.value_php_type(object)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return lower_mixed_prop_set(ctx, object, value, property);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_stdclass_prop_set(ctx, object, value, property);
    }
    if let Some(offset) = dynamic_property_hash_offset_for_object(ctx, object, property)? {
        return lower_allow_dynamic_prop_set(ctx, object, value, property, offset, inst.span);
    }
    let slot = resolve_property_slot(ctx, object, property, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    emit_property_store(ctx, value, &slot, base_reg)
}

/// Lowers a runtime-name write to a statically known stdClass receiver.
pub(super) fn lower_runtime_stdclass_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_pop_reg(ctx.emitter, "x3");
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
            abi::emit_pop_reg(ctx.emitter, "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    Ok(())
}

/// Lowers a runtime-name write to a known class by comparing against declared slots.
pub(super) fn lower_runtime_object_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    class_name: &str,
    inst: &Instruction,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    let slots = declared_dynamic_property_set_slots(ctx, class_name, value, inst)?;
    let match_labels = slots
        .iter()
        .map(|slot| ctx.next_label(&format!("dyn_prop_set_{}", label_fragment(&slot.property))))
        .collect::<Vec<_>>();
    let miss_label = ctx.next_label("dyn_prop_set_miss");
    let done_label = ctx.next_label("dyn_prop_set_done");

    let object_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    abi::emit_push_reg(ctx.emitter, object_reg);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    for (slot, label) in slots.iter().zip(match_labels.iter()) {
        emit_branch_if_dynamic_name_matches(ctx, &slot.property, label);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    for (slot, label) in slots.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, 16);
        emit_property_store(ctx, value, slot, base_reg)?;
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a runtime-name write when the receiver is a boxed Mixed object.
pub(super) fn lower_runtime_mixed_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    let candidates = declared_mixed_property_set_candidates(ctx, value, inst)?;
    let done_label = ctx.next_label("mixed_dyn_prop_set_done");
    let miss_label = ctx.next_label("mixed_dyn_prop_set_miss");
    let stdclass_label = ctx.next_label("mixed_dyn_prop_set_stdclass");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_dyn_prop_set_{}",
                label_fragment(&candidate.slot.property)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_not_object(ctx, &done_label);
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
        emit_property_store(ctx, value, &candidate.slot, base_reg)?;
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&stdclass_label);
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    emit_runtime_stdclass_set_for_stacked_name(ctx, value, &value_ty, 16, 0)?;
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Resolves declared slots on a known object class that can accept this value.
pub(super) fn declared_dynamic_property_set_slots(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    value: ValueId,
    inst: &Instruction,
) -> Result<Vec<PropertySlot>> {
    let value_ty = ctx.value_php_type(value)?;
    let normalized = class_name.trim_start_matches('\\');
    let property_names = {
        let class_info =
            ctx.module.class_infos.get(normalized).ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", normalized))
            })?;
        class_info
            .properties
            .iter()
            .enumerate()
            .filter(|(index, (property, _))| {
                class_info.visible_property_index(property) == Some(*index)
            })
            .filter(|(_, (property, _))| {
                class_info
                    .property_visibilities
                    .get(property)
                    .unwrap_or(&Visibility::Public)
                    == &Visibility::Public
            })
            .filter(|(_, (property, _))| {
                let getter = php_symbol_key(&property_hook_get_method(property));
                let setter = php_symbol_key(&property_hook_set_method(property));
                !class_info.readonly_properties.contains(property)
                    && !(class_info.methods.contains_key(&getter)
                        && !class_info.methods.contains_key(&setter))
            })
            .map(|(_, (property, _))| property.clone())
            .collect::<Vec<_>>()
    };
    let mut slots = Vec::new();
    for property in property_names {
        let slot = resolve_property_slot_for_class(ctx, normalized, &property, inst)?;
        if ensure_property_value_supported(ctx, &slot, value, &value_ty, inst).is_ok() {
            slots.push(slot);
        }
    }
    Ok(slots)
}

/// Collects Mixed receiver declared-property candidates that can accept this value.
pub(super) fn declared_mixed_property_set_candidates(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    inst: &Instruction,
) -> Result<Vec<MixedPropertyCandidate>> {
    let value_ty = ctx.value_php_type(value)?;
    let mut candidates = Vec::new();
    let mut sorted_classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in sorted_classes {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
            continue;
        }
        for (property, _) in &class_info.properties {
            let Ok(slot) = resolve_property_slot_for_class(ctx, class_name, property, inst) else {
                continue;
            };
            if ensure_property_value_supported(ctx, &slot, value, &value_ty, inst).is_err() {
                continue;
            }
            candidates.push(MixedPropertyCandidate {
                class_id: class_info.class_id,
                slot,
            });
        }
    }
    candidates.sort_by(|left, right| {
        left.class_id
            .cmp(&right.class_id)
            .then_with(|| left.slot.property.cmp(&right.slot.property))
    });
    Ok(candidates)
}

/// Branches to `target_label` when the unboxed Mixed result is not an object.
pub(super) fn emit_branch_if_mixed_unboxed_not_object(ctx: &mut FunctionContext<'_>, target_label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // check whether the boxed receiver holds an object payload
            ctx.emitter.instruction(&format!("b.ne {}", target_label));         // non-object dynamic property writes are ignored
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // check whether the boxed receiver holds an object payload
            ctx.emitter.instruction(&format!("jne {}", target_label));          // non-object dynamic property writes are ignored
        }
    }
}

/// Pushes the object payload returned by `__rt_mixed_unbox` onto the temp stack.
pub(super) fn push_mixed_unboxed_object_payload(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg(ctx.emitter, "x1"),
        Arch::X86_64 => abi::emit_push_reg(ctx.emitter, "rdi"),
    }
}

/// Branches when both the stacked object class id and runtime property name match.
pub(super) fn emit_branch_if_mixed_dynamic_property_candidate_matches(
    ctx: &mut FunctionContext<'_>,
    candidate: &MixedPropertyCandidate,
    matched_label: &str,
) {
    let next_label = ctx.next_label("mixed_dyn_prop_set_next");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 16);
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the candidate receiver class id
            abi::emit_load_int_immediate(ctx.emitter, "x11", candidate.class_id as i64);
            ctx.emitter.instruction("cmp x10, x11");                            // compare receiver class id before checking the property name
            ctx.emitter.instruction(&format!("b.ne {}", next_label));           // skip name comparison for unrelated classes
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", 16);
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // load the candidate receiver class id
            abi::emit_load_int_immediate(ctx.emitter, "r12", candidate.class_id as i64);
            ctx.emitter.instruction("cmp r10, r12");                            // compare receiver class id before checking the property name
            ctx.emitter.instruction(&format!("jne {}", next_label));            // skip name comparison for unrelated classes
        }
    }
    emit_branch_if_dynamic_name_matches(ctx, &candidate.slot.property, matched_label);
    ctx.emitter.label(&next_label);
}

/// Branches when a stacked object payload is a stdClass instance.
pub(super) fn emit_branch_if_stacked_object_is_stdclass(
    ctx: &mut FunctionContext<'_>,
    object_stack_offset: usize,
    matched_label: &str,
) {
    let Some(stdclass_id) = stdclass_class_id(ctx) else {
        return;
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", object_stack_offset);
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the stacked object's class id
            abi::emit_load_int_immediate(ctx.emitter, "x11", stdclass_id as i64);
            ctx.emitter.instruction("cmp x10, x11");                            // check whether the runtime receiver is stdClass
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // route stdClass writes through the dynamic-property helper
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", object_stack_offset);
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // load the stacked object's class id
            abi::emit_load_int_immediate(ctx.emitter, "r12", stdclass_id as i64);
            ctx.emitter.instruction("cmp r10, r12");                            // check whether the runtime receiver is stdClass
            ctx.emitter.instruction(&format!("je {}", matched_label));          // route stdClass writes through the dynamic-property helper
        }
    }
}

/// Calls `__rt_stdclass_get` using a stacked object pointer and runtime name pair.
pub(super) fn emit_runtime_stdclass_get_for_stacked_name(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object_stack_offset: usize,
    name_stack_offset: usize,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", object_stack_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", name_stack_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", name_stack_offset + 8);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", object_stack_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", name_stack_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", name_stack_offset + 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())
}

/// Calls `__rt_stdclass_set` using a stacked object pointer and runtime name pair.
pub(super) fn emit_runtime_stdclass_set_for_stacked_name(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
    object_stack_offset: usize,
    name_stack_offset: usize,
) -> Result<()> {
    materialize_dynamic_property_mixed_value(ctx, value, value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", object_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", name_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", name_stack_offset + 24);
            abi::emit_pop_reg(ctx.emitter, "x3");
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", object_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", name_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", name_stack_offset + 24);
            abi::emit_pop_reg(ctx.emitter, "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    Ok(())
}

/// Lowers `unset($object->property)` for a declared, accessible instance property.
///
/// PHP removes the property from the instance; a *typed* property becomes
/// "uninitialized" again. elephc renders declared properties from a fixed per-class
/// descriptor and cannot drop a slot, so the slot is stamped with the shared
/// uninitialized-typed-property marker — exactly the state a typed property without
/// a default starts in. `isset()` then answers false, `print_r`/`var_export` skip the
/// property, and a later read raises the "must not be accessed before initialization"
/// diagnostic. Any refcounted payload the slot owned is released first, so the write
/// cannot leak a string/array/object.
///
/// A property that lives in the receiver's DYNAMIC-property hash instead of a fixed
/// slot — every `stdClass` property, and an undeclared name on an
/// `#[AllowDynamicProperties]` class — is genuinely removable, so it takes the hash
/// removal path and matches PHP exactly: the key disappears, `isset()` answers false,
/// the value renderers stop listing it, and a later write re-appends it.
///
/// Every other slot shape is REFUSED rather than silently skipped. A by-reference
/// property slot holds an object-owned ref-cell pointer that the destructor still has
/// to free and that a later write would write THROUGH — reviving the alias PHP's
/// `unset()` just broke — so neither zeroing nor keeping the cell reproduces PHP.
/// A packed field and an undeclared slot have no removable storage at all. Skipping
/// them quietly left `isset()` answering `true` after an `unset()`, so they now name
/// themselves instead.
pub(in crate::codegen::lower_inst) fn lower_prop_unset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    if let Some(hash_offset) = dynamic_property_hash_offset_for_object(ctx, object, &property)? {
        return lower_dynamic_prop_unset(ctx, object, &property, hash_offset);
    }
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    if let Some(reason) = unset_unsupported_slot_reason(&slot) {
        return Err(CodegenIrError::unsupported(format!(
            "unset() of {} {}::${}",
            reason, slot.class_name, slot.property
        )));
    }
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    release_previous_property_value(ctx, base_reg, &slot.php_type, slot.offset, None);
    emit_property_uninitialized_marker(ctx, &slot, base_reg);
    Ok(())
}
