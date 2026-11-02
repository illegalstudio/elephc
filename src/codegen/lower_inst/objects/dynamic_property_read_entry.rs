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
    let read_done = emit_property_read_state_guard(
        ctx,
        &slot,
        base_reg,
        property_fetch_mode(inst),
        PropertyReadMissingResult::Instruction(inst),
    )?;
    emit_property_load(ctx, &slot, base_reg)?;
    materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
    if let Some(read_done) = read_done {
        ctx.emitter.label(&read_done);
    }
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
    // A receiver typed with a class the closed world never declared -- a builtin elephc does not
    // model, such as `SimpleXMLElement` -- can only hold a value no compiled code produced. The
    // read fails at run time, when it actually executes, instead of refusing the whole body:
    // that refusal deferred Symfony's validator `XmlFileLoader` and cost the demo a whole second
    // compile.
    if let PhpType::Object(class_name) = ctx.value_php_type(object)?.codegen_repr() {
        let normalized = class_name.trim_start_matches('\\');
        if !ctx.module.class_infos.contains_key(normalized)
            && !crate::types::checker::builtin_stdclass::is_stdclass(normalized)
        {
            super::super::exceptions::emit_error(
                ctx,
                &format!("Class \"{normalized}\" is not supported by this compiled program"),
            );
            super::super::arrays::emit_array_get_null_fallback(
                ctx,
                &inst.result_php_type.codegen_repr(),
                false,
            );
            return store_if_result(ctx, inst);
        }
    }
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
    emit_property_warning_fragment(ctx, b"Warning: Attempt to read property \"", false);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.load_string_value_to_regs(property_value, "x1", "x2")?,
        Arch::X86_64 => ctx.load_string_value_to_regs(property_value, "rdi", "rsi")?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    emit_property_warning_fragment(ctx, b"\" on null\n", true);
    Ok(())
}

/// Writes one static fragment through the suppressible PHP warning channel.
pub(super) fn emit_property_warning_fragment(ctx: &mut FunctionContext<'_>, bytes: &[u8], complete: bool) {
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
    abi::emit_call_label(ctx.emitter, if complete { "__rt_diag_warning" } else { "__rt_diag_warning_fragment" });
}

/// Appends PHP's ` in FILE on line N` suffix to the pending diagnostic.
///
/// The static message fragment already sits in the warning buffer; these raw writes add the
/// location PHP's CLI formatter reports for a statement in the main script. The caller finishes
/// the diagnostic with its newline fragment so one warning is dispatched exactly once.
pub(super) fn emit_property_warning_location(ctx: &mut FunctionContext<'_>, line: u32) {
    // Record where the message ends before the location suffix is appended. Warning
    // dispatch hands PHP error handlers only that prefix, while the default output keeps
    // the complete buffered line including ` in <file> on line <n>`.
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, "_rt_diag_pending_len", 0);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_rt_diag_warning_message_len", 0);
    emit_property_warning_fragment(ctx, b" in ", false);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", "_script_source_file");
            abi::emit_load_symbol_to_reg(ctx.emitter, "x2", "_script_source_file_len", 0);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", "_script_source_file");
            abi::emit_load_symbol_to_reg(ctx.emitter, "rsi", "_script_source_file_len", 0);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    emit_property_warning_fragment(ctx, b" on line ", false);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), i64::from(line));
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                               // pass the formatted line-number pointer to the warning fragment buffer
        ctx.emitter.instruction("mov rsi, rdx");                               // pass the formatted line-number length to the warning fragment buffer
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    // Publish the real caller location for a user error handler: one `php_site` record
    // `{file descriptor, line}`, whose zero descriptor names the entry file (see
    // `prepare_call_site`); warning dispatch expands it into the `_php_diagnostic_*` words.
    let record = ctx.data.add_words(vec![
        crate::codegen_support::data_section::DataWord::U64(0),
        crate::codegen_support::data_section::DataWord::U64(u64::from(line)),
    ]);
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, scratch, &record);
    let site = abi::php_site_symbol(&ctx.emitter.target);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, &site, 0);
}

/// Answers an `isset()`/`empty()` probe that found no property.
///
/// The checker types the probe with `__get`'s return type, which may be a scalar with no null
/// spelling; there the only consumer is the emptiness test, so the type's empty value stands in
/// for null (`0`, `false`, `0.0`, `""`).
fn emit_absent_probe_result(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    match inst.result_php_type.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction("fmov d0, xzr"),           // an absent property probes as 0.0
            Arch::X86_64 => ctx.emitter.instruction("xorpd xmm0, xmm0"),        // an absent property probes as 0.0
        },
        PhpType::Str => {
            let (pointer, length) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, pointer, 0);
            abi::emit_load_int_immediate(ctx.emitter, length, 0);
        }
        _ => emit_dynamic_property_miss_result(ctx, inst),
    }
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
    let mode = property_fetch_mode(inst);
    if let Some(class_name) = magic_get_receiver_class(ctx, object, property)? {
        // `isset()`/`empty()` on a class with `__get` but no `__isset` find nothing and never
        // call `__get`: the probe answers null.
        let skips_get = mode.skips_get_without_isset()
            && !ctx.module.class_infos.get(class_name.trim_start_matches('\\')).is_some_and(
                |info| info.methods.contains_key(&php_symbol_key("__isset")),
            );
        if skips_get {
            emit_absent_probe_result(ctx, inst);
            return store_if_result(ctx, inst);
        }
        return lower_magic_get_prop(ctx, inst, object, &class_name, property);
    }
    // Same runtime-class rule as the `PropGet` sibling: a name php does not answer from a declared
    // slot on the STATIC class is decided per runtime class, kind and all.
    if let Some(plan) = dynamic_property_runtime_plan_for_object(
        ctx,
        object,
        property,
        PropertyAccessKind::DirectRead(mode),
        inst,
    )? {
        return emit_object_property_runtime_dispatch(
            ctx,
            object,
            &plan,
            "const_dyn_prop_get",
            DispatchStackCleanup::NONE,
            |ctx, arm| emit_dynamic_plan_read(ctx, inst, object, property, arm, mode),
        );
    }
    // A literal name reaches an inaccessible slot only where the checker tolerates the access.
    // php raises for a value read and answers a probe `null`; neither may read the slot, and on a
    // class that declares an accessor php reports nothing at all.
    if let PhpType::Object(class_name) = ctx.value_php_type(object)? {
        match resolve_property_read_arm(ctx, &class_name, property, mode, inst)? {
            // Only a value read reaches this arm; a probe drops it below.
            Some(PropertyNameArm::Refuse { message, .. }) => {
                super::super::exceptions::emit_error(ctx, &message);
                return Ok(());
            }
            // `MagicDeferred` is php's accessor answer this compiler cannot dispatch yet, and
            // `None` is a probe of a refused name. Both answer php `null` without reading the
            // slot, which holds storage this scope may not see.
            Some(PropertyNameArm::MagicDeferred) | None => {
                emit_dynamic_property_miss_result(ctx, inst);
                return store_if_result(ctx, inst);
            }
            _ => {}
        }
    }
    let slot = resolve_property_slot(ctx, object, property, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    emit_typed_pointer_receiver_null_check(ctx, object, base_reg)?;
    let read_done = emit_property_read_state_guard(
        ctx,
        &slot,
        base_reg,
        property_fetch_mode(inst),
        PropertyReadMissingResult::Instruction(inst),
    )?;
    emit_property_load(ctx, &slot, base_reg)?;
    materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
    if let Some(read_done) = read_done {
        ctx.emitter.label(&read_done);
    }
    store_if_result(ctx, inst)
}

/// The class name of every class id, for recognising refusals that name the receiver's class.
pub(super) fn class_names_by_id(ctx: &FunctionContext<'_>) -> crate::fast_hash::FastMap<u64, String> {
    ctx.module
        .class_infos
        .iter()
        .map(|(name, info)| (info.class_id, name.clone()))
        .collect()
}

/// The visibility word of a read refusal that is plain php text naming the arm's own class.
///
/// Such an arm needs no per-class code: the text is composed from the receiver's class name at run
/// time. A class that keeps eval-created properties separately takes the longer probe in
/// `emit_refused_read_or_eval_child_property` instead, so it is left alone.
pub(super) fn read_arm_dynamic_refusal(
    ctx: &FunctionContext<'_>,
    class_names: &crate::fast_hash::FastMap<u64, String>,
    candidate: &MixedPropertyReadCandidate,
) -> Option<&'static str> {
    let MixedPropertyReadKind::Refuse(message) = &candidate.kind else {
        return None;
    };
    let slot = &candidate.candidate.slot;
    if ctx
        .module
        .class_infos
        .get(slot.class_name.trim_start_matches('\\'))
        .is_some_and(|info| info.eval_property_storage)
    {
        return None;
    }
    super::runtime_property_writes::refusal_message_visibility(
        class_names,
        candidate.candidate.class_id,
        &slot.property,
        message,
    )
}

/// For each read arm, the index of the arm whose code it shares.
///
/// Slot reads that run the same code share the first one's label and body: a subclass that
/// inherits a slot unchanged reads it exactly as its parent does, and php names the DECLARING
/// class in the `must not be accessed before initialization` error. Plain visibility refusals
/// share one body per visibility word.
pub(super) fn shared_read_arm_owners(
    ctx: &FunctionContext<'_>,
    class_names: &crate::fast_hash::FastMap<u64, String>,
    candidates: &[MixedPropertyReadCandidate],
) -> Vec<usize> {
    let grouping = std::env::var_os("ELEPHC_NO_ARM_GROUPS").is_none();
    let mut first_for_key: crate::fast_hash::FastMap<String, usize> = Default::default();
    let mut owners = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let slot = &candidate.candidate.slot;
        let key = if !grouping {
            format!("unique|{index}")
        } else if let Some(visibility) = read_arm_dynamic_refusal(ctx, class_names, candidate) {
            format!("refuse-dynamic|{visibility}")
        } else if matches!(candidate.kind, MixedPropertyReadKind::Slot) {
            format!(
                "slot|{}|{}|{:?}|{}|{}|{}|{}",
                super::runtime_property_writes::message_class_for_slot(ctx, slot),
                slot.property,
                slot.php_type,
                slot.offset,
                slot.is_declared,
                slot.is_packed,
                slot.is_reference
            )
        } else {
            format!("unique|{index}")
        };
        owners.push(*first_for_key.entry(key).or_insert(index));
    }
    owners
}

/// For each class-hash read arm, the index of the first arm with the same hash offset.
pub(super) fn shared_hash_arm_owners(arms: &[MixedClassHashArm]) -> Vec<usize> {
    let grouping = std::env::var_os("ELEPHC_NO_ARM_GROUPS").is_none();
    let mut first_for_offset: crate::fast_hash::FastMap<usize, usize> = Default::default();
    arms.iter()
        .enumerate()
        .map(|(index, arm)| {
            if grouping {
                *first_for_offset.entry(arm.hash_offset).or_insert(index)
            } else {
                index
            }
        })
        .collect()
}

/// Lowers a runtime-name dynamic property read from a boxed `Mixed` receiver.
pub(super) fn lower_runtime_dynamic_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property_value: ValueId,
) -> Result<()> {
    if try_lower_outlined_mixed_prop_get(ctx, inst, object, property_value)? {
        return Ok(());
    }
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    ensure_dynamic_property_miss_supported(inst)?;
    let mode = property_fetch_mode(inst);
    let candidates = declared_mixed_property_get_candidates(ctx, mode, inst, None)?;
    let done_label = ctx.next_label("mixed_dyn_prop_get_done");
    let miss_label = ctx.next_label("mixed_dyn_prop_get_miss");
    let miss_no_stack_label = ctx.next_label("mixed_dyn_prop_get_miss_no_stack");
    let eval_done_label = ctx.next_label("mixed_dyn_prop_get_eval_done");
    let stdclass_label = ctx.next_label("mixed_dyn_prop_get_stdclass");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_dyn_prop_get_{}",
                label_fragment(&candidate.candidate.slot.property)
            ))
        })
        .collect::<Vec<_>>();
    // A runtime name a class does not DECLARE can still live in that class's own per-instance
    // hash. Without an arm for it the name reached a miss path that understands `stdClass` alone,
    // so an `#[\AllowDynamicProperties]` user class answered `null` for a key it really held.
    // The arms are per CLASS, because the name is only known at run time; the declared (class,
    // name) probes above run first, so a name the class declares keeps its declared answer.
    let class_names = class_names_by_id(ctx);
    let arm_owner = shared_read_arm_owners(ctx, &class_names, &candidates);
    let match_labels = arm_owner.iter().map(|owner| match_labels[*owner].clone()).collect::<Vec<_>>();
    let hash_arms = mixed_class_hash_arms(ctx, "", &[])?;
    let hash_labels = hash_arms
        .iter()
        .map(|arm| {
            ctx.next_label(&format!(
                "mixed_dyn_prop_get_hash_{}",
                label_fragment(&arm.class_name)
            ))
        })
        .collect::<Vec<_>>();

    // The hash read depends on the class's hash offset alone, so classes with the same offset
    // share one body.
    let hash_owner = shared_hash_arm_owners(&hash_arms);
    let hash_labels = hash_owner.iter().map(|owner| hash_labels[*owner].clone()).collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_not_object(ctx, &miss_no_stack_label);
    push_mixed_unboxed_object_payload(ctx);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    let dispatch_candidates = candidates
        .iter()
        .map(|candidate| candidate.candidate.clone())
        .collect::<Vec<_>>();
    emit_mixed_dynamic_property_candidate_dispatch(ctx, &dispatch_candidates, &match_labels);
    for (arm, label) in hash_arms.iter().zip(hash_labels.iter()) {
        emit_branch_if_stacked_object_class_matches(ctx, arm.class_id, 16, label);
    }
    emit_branch_if_stacked_object_is_stdclass(ctx, 16, &stdclass_label);
    abi::emit_jump(ctx.emitter, &miss_label);

    for (index, (arm, label)) in hash_arms.iter().zip(hash_labels.iter()).enumerate() {
        if hash_owner[index] != index {
            continue;
        }
        ctx.emitter.label(label);
        // The arm matched this runtime class, so the offset is its own. The helper takes the
        // runtime key from the stacked block and releases that block itself.
        lower_runtime_allow_dynamic_prop_get(ctx, inst, arm.hash_offset, 16, 0, 32)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    for (index, (candidate, label)) in candidates.iter().zip(match_labels.iter()).enumerate() {
        if arm_owner[index] != index {
            continue;
        }
        ctx.emitter.label(label);
        if let Some(visibility) = read_arm_dynamic_refusal(ctx, &class_names, candidate) {
            // Composes the text from the receiver's own class name, releasing the block.
            super::closure_scope_gate::emit_scope_refusal(ctx, visibility);
            continue;
        }
        let mut shared_slot = candidate.candidate.slot.clone();
        if matches!(candidate.kind, MixedPropertyReadKind::Slot) {
            shared_slot.class_name = super::runtime_property_writes::message_class_for_slot(ctx, &shared_slot);
        }
        let slot = &shared_slot;
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, 16);
        // Only `Slot` touches storage. Every temporary block is released BEFORE the arm's own
        // exit, so the unwinder and `done` both see the same stack pointer on every path.
        match &candidate.kind {
            // php refuses the read from this scope.
            MixedPropertyReadKind::Refuse(message) => {
                abi::emit_release_temporary_stack(ctx.emitter, 32);
                emit_refused_read_or_eval_child_property(
                    ctx, slot, message, base_reg, mode, &done_label,
                )?;
                continue;
            }
            // php makes this name invisible on this class, so it answers from the per-instance
            // hash and a value read that finds nothing warns `Undefined property`. The shared miss
            // arm cannot do it: it answers for every unmatched name, including the ones this
            // compiler still reports silently.
            MixedPropertyReadKind::ScopeDynamic => {
                let class_name = slot.class_name.clone();
                let property = slot.property.clone();
                abi::emit_release_temporary_stack(ctx.emitter, 32);
                emit_scope_dynamic_property_read(
                    ctx, inst, &class_name, &property, base_reg, mode,
                )?;
                abi::emit_jump(ctx.emitter, &done_label);
                continue;
            }
            // php answers `__get` or `__isset`, which a runtime name cannot reach yet.
            MixedPropertyReadKind::MagicDeferred => {
                abi::emit_release_temporary_stack(ctx.emitter, 32);
                emit_dynamic_property_miss_result(ctx, inst);
                abi::emit_jump(ctx.emitter, &done_label);
                continue;
            }
            MixedPropertyReadKind::Slot => {}
        }
        let read_done = emit_property_read_state_guard(
            ctx,
            slot,
            base_reg,
            property_fetch_mode(inst),
            PropertyReadMissingResult::Instruction(inst),
        )?;
        emit_property_load(ctx, slot, base_reg)?;
        materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
        if let Some(read_done) = read_done {
            ctx.emitter.label(&read_done);
        }
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    // An object of an eval-declared class is carried by a stdClass runtime object (or matches no
    // compiled class at all), so both arrival points ask the owning eval context before the native
    // hash answers null: `$object->{$name}` on an eval model reaches its attributes and `__get()`.
    let eval_fallback = super::mixed_property_reads::eval_owner_fallback_available(ctx);

    ctx.emitter.label(&stdclass_label);
    if eval_fallback {
        let native_stdclass_label = ctx.next_label("mixed_dyn_prop_get_stdclass_native");
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        builtins::lower_eval_owner_mixed_dynamic_property_get(
            ctx,
            object,
            property_value,
            &native_stdclass_label,
            &eval_done_label,
        )?;
        ctx.emitter.label(&native_stdclass_label);
        ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        push_mixed_unboxed_object_payload(ctx);
        let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
        ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
        abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    }
    emit_runtime_stdclass_get_for_stacked_name(ctx, inst, 16, 0)?;
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    if eval_fallback {
        let native_miss_label = ctx.next_label("mixed_dyn_prop_get_miss_native");
        builtins::lower_eval_owner_mixed_dynamic_property_get(
            ctx,
            object,
            property_value,
            &native_miss_label,
            &eval_done_label,
        )?;
        ctx.emitter.label(&native_miss_label);
    }
    emit_dynamic_property_miss_result(ctx, inst);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_no_stack_label);
    if eval_fallback {
        let native_no_stack_label = ctx.next_label("mixed_dyn_prop_get_miss_no_stack_native");
        builtins::lower_eval_owner_mixed_dynamic_property_get(
            ctx,
            object,
            property_value,
            &native_no_stack_label,
            &eval_done_label,
        )?;
        ctx.emitter.label(&native_no_stack_label);
    }
    emit_dynamic_property_miss_result(ctx, inst);

    ctx.emitter.label(&done_label);
    ctx.emitter.label(&eval_done_label);
    store_if_result(ctx, inst)
}
