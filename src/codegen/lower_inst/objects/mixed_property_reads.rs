//! Purpose:
//! Lowers property reads from boxed Mixed and union receivers.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Runtime class dispatch produces owned Mixed results and preserves null warnings.

use super::*;

/// Lowers a property read from a raw object whose static type is polymorphic.
///
/// Interface or abstract-parent typing identifies the contract but not the concrete object layout.
/// Dispatch by runtime class id across every AOT implementation that owns the requested property,
/// then read that implementation's authoritative slot.
pub(super) fn lower_polymorphic_object_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    target_name: &str,
    property: &str,
) -> Result<()> {
    let object_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    lower_polymorphic_object_prop_get_from_loaded_object(ctx, inst, target_name, property)
}

/// Dispatches a property read after the raw object payload has been loaded into the result register.
fn lower_polymorphic_object_prop_get_from_loaded_object(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target_name: &str,
    property: &str,
) -> Result<()> {
    let candidates = polymorphic_property_candidates(ctx, target_name, property, inst)?;
    // No candidate is a valid answer: the classes declaring the name were pruned, so every live
    // object takes the miss arm, which is php's `Undefined property`.
    let miss_label = ctx.next_label("interface_prop_miss");
    let done_label = ctx.next_label("interface_prop_done");
    let stdclass_label = target_name
        .trim_start_matches('\\')
        .is_empty()
        .then(|| ctx.next_label("generic_object_stdclass_prop"));
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "interface_prop_{}",
                label_fragment(&candidate.slot.class_name)
            ))
        })
        .collect::<Vec<_>>();

    emit_raw_object_property_class_dispatch(
        ctx,
        &candidates,
        &match_labels,
        stdclass_label.as_deref(),
        &miss_label,
    );

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let base_reg = abi::int_result_reg(ctx.emitter);
        let read_done = if candidate.slot.is_declared {
            emit_property_read_state_guard(
                ctx,
                &candidate.slot,
                base_reg,
                PropertyFetchMode::Read,
                PropertyReadMissingResult::Instruction(inst),
            )?
        } else {
            None
        };
        emit_property_load(ctx, &candidate.slot, base_reg)?;
        materialize_loaded_property_result(ctx, inst, &candidate.slot.php_type)?;
        if let Some(read_done) = read_done {
            ctx.emitter.label(&read_done);
        }
        store_if_result(ctx, inst)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    if let Some(stdclass_label) = stdclass_label {
        ctx.emitter.label(&stdclass_label);
        emit_stdclass_get_from_loaded_object(ctx, property);
        cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
        store_if_result(ctx, inst)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&miss_label);
    emit_dynamic_property_miss_result(ctx, inst);
    store_if_result(ctx, inst)?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Collects concrete property slots for classes satisfying one interface or parent-class target.
fn polymorphic_property_candidates(
    ctx: &FunctionContext<'_>,
    target_name: &str,
    property: &str,
    inst: &Instruction,
) -> Result<Vec<MixedPropertyCandidate>> {
    let normalized_target = target_name.trim_start_matches('\\');
    let target_key = php_symbol_key(normalized_target);
    let target_is_any_object = normalized_target.is_empty();
    let target_is_interface = ctx
        .module
        .interface_infos
        .keys()
        .any(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == target_key);
    let mut candidates = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        let satisfies_target = if target_is_any_object {
            true
        } else if target_is_interface {
            object_type_implements_interface(ctx, class_name, target_name)
        } else {
            php_symbol_key(class_name.trim_start_matches('\\')) == target_key
                || class_extends_class(ctx, class_name, target_name)
        };
        let backing = super::property_resolution::reflection_backing(ctx, class_name, property)
        .unwrap_or(property);
        if !satisfies_target
            || !class_info
                .properties
                .iter()
                .any(|(name, _)| name == backing)
        {
            continue;
        }
        let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
        if target_is_any_object
            && !generic_property_candidate_matches_result(ctx, &slot.php_type, &inst.result_php_type)
        {
            continue;
        }
        candidates.push(MixedPropertyCandidate {
            class_id: class_info.class_id,
            slot,
        });
    }
    candidates.sort_by_key(|candidate| candidate.class_id);
    Ok(candidates)
}

/// Returns whether one concrete property slot can materialize the generic EIR result shape.
fn generic_property_candidate_matches_result(
    ctx: &FunctionContext<'_>,
    source: &PhpType,
    result: &PhpType,
) -> bool {
    let source = source.codegen_repr();
    let result = result.codegen_repr();
    if result == PhpType::Mixed || source == result {
        return true;
    }
    match (&source, &result) {
        (PhpType::Object(source_name), PhpType::Object(result_name)) => {
            source_name.trim_start_matches('\\').is_empty()
                || object_type_is_a(ctx, source_name, result_name)
        }
        _ => false,
    }
}

/// Emits class-id branches for a raw interface object already loaded in the result register.
fn emit_raw_object_property_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    candidates: &[MixedPropertyCandidate],
    match_labels: &[String],
    stdclass_label: Option<&str>,
    miss_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the concrete class id behind the interface value
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare against one AOT interface implementor
                ctx.emitter.instruction(&format!("b.eq {}", label));            // read that implementor's declared property slot
            }
            if let Some(stdclass_label) = stdclass_label {
                emit_branch_to_stdclass_candidate(ctx, "x9", "x10", stdclass_label);
            }
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r11, QWORD PTR [rax]");                // load the concrete class id behind the interface value
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare against one AOT interface implementor
                ctx.emitter.instruction(&format!("je {}", label));              // read that implementor's declared property slot
            }
            if let Some(stdclass_label) = stdclass_label {
                emit_branch_to_stdclass_candidate(ctx, "r11", "r10", stdclass_label);
            }
        }
    }
    abi::emit_jump(ctx.emitter, miss_label);
}

/// Lowers a declared-property read from a boxed union that may hold one known object class.
pub(super) fn lower_union_object_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
    let object_label = ctx.next_label("union_prop_object");
    let done_label = ctx.next_label("union_prop_done");
    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_object(ctx, &object_label);
    emit_dynamic_property_miss_result(ctx, inst);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&object_label);
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    move_mixed_unboxed_object_payload(ctx, base_reg);
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
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `$mixed->property` through the shared stdClass-aware runtime helper.
pub(super) fn lower_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    if builtins::has_eval_context(ctx) {
        return builtins::lower_eval_property_get(ctx, inst, object, property);
    }
    let mode = property_fetch_mode(inst);
    let candidates = declared_mixed_property_candidates(ctx, property, mode, inst)?;
    // A user class may answer this name exclusively from its per-instance property hash. Such
    // a class contributes no declared candidate, but still needs the class-id ladder below. A
    // direct fallback to the stdClass-shaped helper would lose that class altogether.
    let hash_arms = read_hash_arms_without_magic(ctx, mixed_class_hash_arms(ctx, property, &[])?, mode);
    let hash_arm_count = hash_arms.len();
    let has_hash_arms = hash_arm_count > 0;
    // A class that declares `__get` but no such property answers from the accessor, so it needs
    // an arm of its own even though it contributes no declared candidate.
    let magic_groups = mixed_magic_get_groups(ctx, property, mode, &candidates, &hash_arms)?;
    if super::outlined_dynamic_property::try_lower_outlined_named_mixed_prop_get(
        ctx,
        inst,
        object,
        property,
        candidates.len() + hash_arm_count + magic_groups.len(),
    )? {
        return Ok(());
    }
    if !candidates.is_empty() || has_hash_arms || !magic_groups.is_empty() {
        return lower_declared_mixed_prop_get(ctx, inst, object, property, candidates, mode);
    }
    lower_runtime_mixed_prop_get(ctx, inst, object, property)
}

/// Lowers a `Mixed` receiver by dispatching known user classes before stdClass fallback.
pub(super) fn lower_declared_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
    candidates: Vec<MixedPropertyReadCandidate>,
    mode: PropertyFetchMode,
) -> Result<()> {
    let null_label = ctx.next_label("mixed_prop_null");
    let miss_label = ctx.next_label("mixed_prop_miss");
    let done_label = ctx.next_label("mixed_prop_done");
    let stdclass_label = ctx.next_label("mixed_prop_stdclass");
    let mut match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_prop_{}",
                label_fragment(&candidate.candidate.slot.class_name)
            ))
        })
        .collect::<Vec<_>>();
    let arm_owner = super::dynamic_property_read_entry::shared_slot_arm_owners(ctx, &candidates);
    for (index, owner) in arm_owner.iter().enumerate() {
        match_labels[index] = match_labels[*owner].clone();
    }
    let mut dispatch = candidates
        .iter()
        .map(|candidate| candidate.candidate.class_id)
        .collect::<Vec<_>>();
    // A class that keeps this name in its own per-instance hash gets an arm too, even though it
    // declares no slot for it. Without one an `#[\AllowDynamicProperties]` user class fell into
    // the miss path, which understands `stdClass` alone, and could not read back what a write
    // through the same Mixed receiver had just stored in its hash.
    let hash_arms =
        read_hash_arms_without_magic(ctx, mixed_class_hash_arms(ctx, property, &dispatch)?, mode);
    // Most of those arms differ only by their class's hash offset, which `_class_dyn_hash_rows`
    // already records: they are answered by ONE row lookup at the miss label instead of one arm
    // each. Every named-read helper of the Symfony demo carried ~3 300 such arms (40 M
    // instructions, an eighth of the program).
    let (hash_arms, table_routed) = split_table_routed_read_hash_arms(ctx, hash_arms, property);
    for arm in &hash_arms {
        match_labels.push(ctx.next_label(&format!(
            "mixed_prop_hash_{}",
            label_fragment(&arm.class_name)
        )));
        dispatch.push(arm.class_id);
    }
    // Classes answering from `__get` alone: one label per accessor implementation, matched by
    // every class id that inherits it.
    let magic_groups = mixed_magic_get_groups(ctx, property, mode, &candidates, &hash_arms)?;
    let magic_labels = magic_groups
        .iter()
        .map(|group| ctx.next_label(&format!("mixed_prop_magic_{}", label_fragment(&group.class_name))))
        .collect::<Vec<_>>();
    for (group, label) in magic_groups.iter().zip(magic_labels.iter()) {
        for class_id in &group.class_ids {
            match_labels.push(label.clone());
            dispatch.push(*class_id);
        }
    }

    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Object(_)) {
        // A narrowed object receiver (non-null, already guarded) is the raw object pointer the
        // class-id ladder reads.
        ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    } else {
        ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        emit_mixed_object_payload_or_null(ctx, &null_label);
    }
    emit_mixed_property_class_dispatch(
        ctx,
        &dispatch,
        &match_labels,
        &stdclass_label,
        &miss_label,
    );

    for (index, (candidate, label)) in candidates.iter().zip(match_labels.iter()).enumerate() {
        if arm_owner[index] != index {
            continue;
        }
        ctx.emitter.label(label);
        // Only `Slot` touches storage. php refuses the read from this scope, or answers an
        // accessor this compiler cannot dispatch for a runtime name yet; either way the private
        // slot stays unread. Only a value read raises: the probes answer null in silence.
        match &candidate.kind {
            MixedPropertyReadKind::Refuse(message) => {
                let base_reg = abi::int_result_reg(ctx.emitter);
                emit_refused_read_or_eval_child_property(
                    ctx, &candidate.candidate.slot, message, base_reg, mode, &done_label,
                )?;
                continue;
            }
            // php makes this name invisible on this class, so it answers from the per-instance
            // hash and a value read that finds nothing warns `Undefined property`. Answering a
            // flat null here instead made a Mixed receiver unable to read back what a Mixed
            // receiver had just written under the same name.
            MixedPropertyReadKind::ScopeDynamic => {
                // The arm already matched the receiver's runtime class id, so this class IS the
                // runtime class and its own hash offset and name are the exact ones php uses.
                let class_name = candidate.candidate.slot.class_name.clone();
                let property = candidate.candidate.slot.property.clone();
                let base_reg = abi::int_result_reg(ctx.emitter);
                // The arm matched a class that resolves the name to a strict ancestor's private
                // slot, which is the one dynamic shape php DOES warn about on a value read.
                let warn_on_miss = mode.is_read();
                match dynamic_property_hash_offset_for_class(ctx, &class_name, &property)? {
                    Some(hash_offset) => emit_scope_dynamic_property_hash_probe(
                        ctx, &class_name, &property, base_reg, hash_offset, warn_on_miss,
                    )?,
                    None => {
                        if warn_on_miss {
                            emit_undefined_property_warning(ctx, &class_name, &property);
                        }
                        emit_boxed_null(ctx);
                    }
                }
                abi::emit_jump(ctx.emitter, &done_label);
                continue;
            }
            // The class declares an accessor php consults before it reports anything, and the
            // declared slot is not this scope's to read. A value read and `??` ask `__get`;
            // `isset()` would ask `__isset`, which still answers null here.
            MixedPropertyReadKind::MagicDeferred => {
                let class_name = class_name_for_id(ctx, candidate.candidate.class_id);
                emit_mixed_magic_get_arm(ctx, class_name.as_deref(), property, mode, &done_label)?;
                continue;
            }
            MixedPropertyReadKind::Slot => {}
        }
        // The arm may serve a whole subtree, so it names the declaring class, as php does.
        let mut shared_slot = candidate.candidate.slot.clone();
        shared_slot.class_name = super::runtime_property_writes::message_class_for_slot(ctx, &shared_slot);
        let slot = &shared_slot;
        let base_reg = abi::int_result_reg(ctx.emitter);
        let read_done = emit_property_read_state_guard(
            ctx,
            slot,
            base_reg,
            mode,
            PropertyReadMissingResult::Boxed,
        )?;
        emit_property_load(ctx, slot, base_reg)?;
        box_mixed_property_candidate_result(ctx, &slot.php_type);
        if let Some(read_done) = read_done {
            ctx.emitter.label(&read_done);
        }
        abi::emit_jump(ctx.emitter, &done_label);
    }

    // The class-only hash arms, in the same order their labels were appended above. Each answers
    // from THIS class's hash at THIS class's offset, and a miss keeps the shared miss arm's
    // answer, which is php's `Undefined property` warning on a value read and silence on a probe.
    for (arm, label) in hash_arms.iter().zip(match_labels.iter().skip(candidates.len())) {
        ctx.emitter.label(label);
        let base_reg = abi::int_result_reg(ctx.emitter);
        emit_scope_dynamic_property_hash_probe(
            ctx,
            &arm.class_name,
            property,
            base_reg,
            arm.hash_offset,
            mode.is_read(),
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    for (group, label) in magic_groups.iter().zip(magic_labels.iter()) {
        ctx.emitter.label(label);
        emit_mixed_magic_get_arm(ctx, Some(&group.class_name), property, mode, &done_label)?;
    }

    ctx.emitter.label(&stdclass_label);
    if eval_owner_fallback_available(ctx) {
        // An object of an eval-declared class is CARRIED by a stdClass runtime object, so it
        // reaches this label too -- and the hash behind it knows none of the class's own
        // properties or its `__get()`. Ask the owning eval context first, exactly as the
        // no-candidate path does; only a genuine stdClass falls through to its hash. A name some
        // compiled class happens to declare (`name`, `type`...) is what sent a magic-property
        // read like `$column->name` here and made it answer null.
        let native_stdclass_label = ctx.next_label("mixed_prop_stdclass_native");
        builtins::lower_eval_owner_mixed_property_get(
            ctx,
            object,
            property,
            &native_stdclass_label,
            &done_label,
        )?;
        ctx.emitter.label(&native_stdclass_label);
        ctx.load_value_to_result(object)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        emit_mixed_object_payload_or_null(ctx, &null_label);
    }
    emit_stdclass_get_from_loaded_object(ctx, property);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    if !table_routed.is_empty() {
        let outside = table_addressable_classes_not_routed(ctx, &table_routed, &dispatch);
        emit_table_dynamic_hash_read(ctx, property, &outside, &done_label);
    }
    if eval_owner_fallback_available(ctx) {
        // A class no compiled candidate matched may be an eval-declared subclass whose own
        // properties live in its owning eval context.
        let native_miss_label = ctx.next_label("mixed_prop_native_miss");
        builtins::lower_eval_owner_mixed_property_get(
            ctx,
            object,
            property,
            &native_miss_label,
            &done_label,
        )?;
        ctx.emitter.label(&native_miss_label);
        ctx.load_value_to_result(object)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        emit_mixed_object_payload_or_null(ctx, &null_label);
    }
    if mode.is_read() {
        emit_undefined_property_warning_for_loaded_object(ctx, property);
    }
    emit_boxed_null(ctx);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_boxed_null(ctx);

    ctx.emitter.label(&done_label);
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Whether a probe asks `__isset` alone and never `__get`: php's `isset()` (`Isset`, and the
/// older `Probe` spelling of the same silent existence test).
pub(super) fn magic_isset_alone(mode: PropertyFetchMode) -> bool {
    matches!(mode, PropertyFetchMode::Probe | PropertyFetchMode::Isset)
}

/// Whether a read in this mode may end in a `__get` call: a value read, `??`, and `empty()`
/// (which asks `__isset` first and skips `__get` when the class has none).
pub(super) fn magic_calls_get(mode: PropertyFetchMode) -> bool {
    matches!(
        mode,
        PropertyFetchMode::Read | PropertyFetchMode::Coalesce | PropertyFetchMode::Empty
    )
}

/// Classes that share one `__get` implementation and answer the read through it.
pub(super) struct MixedMagicGetGroup {
    /// Every runtime class id that inherits the accessor.
    class_ids: Vec<u64>,
    /// One member of the group, used to resolve the accessor call target.
    class_name: String,
}

/// Returns the name of the class with this runtime id, when the module has one.
fn class_name_for_id(ctx: &FunctionContext<'_>, class_id: u64) -> Option<String> {
    ctx.module
        .class_infos
        .iter()
        .find(|(_, info)| info.class_id == class_id)
        .map(|(name, _)| name.clone())
}

/// Collects the classes a `Mixed` read must answer from `__get` although they declare no slot.
///
/// A class that declares the property keeps its declared arm (`MagicDeferred` there when the slot
/// is not this scope's to read). A class without the property reaches the shared miss arm, which
/// warns `Undefined property` for a value read -- but php asks `__get` first. `isset()` (the
/// `Probe` mode) asks `__isset` instead and is left to the miss arm's silent null.
fn mixed_magic_get_groups(
    ctx: &FunctionContext<'_>,
    property: &str,
    mode: PropertyFetchMode,
    candidates: &[MixedPropertyReadCandidate],
    hash_arms: &[MixedClassHashArm],
) -> Result<Vec<MixedMagicGetGroup>> {
    let get_key = php_symbol_key("__get");
    let isset_key = php_symbol_key("__isset");
    // `isset()` asks `__isset` alone (`emit_mixed_magic_isset_arm`); the other modes ask `__get`.
    let required_key = if magic_isset_alone(mode) { &isset_key } else { &get_key };
    let mut groups: Vec<((String, Option<String>), MixedMagicGetGroup)> = Vec::new();
    // Only a class declaring the accessor can join a group, so the module's method index names
    // the classes to look at (in class-table order), instead of every mixed property read
    // scanning the whole class table with two linear arm searches per class.
    let answered: crate::fast_hash::FastSet<u64> = candidates
        .iter()
        .map(|candidate| candidate.candidate.class_id)
        .chain(hash_arms.iter().map(|arm| arm.class_id))
        .collect();
    let classes_by_method = ctx.module.classes_by_method();
    let declaring = classes_by_method.get(required_key).map(Vec::as_slice).unwrap_or(&[]);
    for class_name in declaring {
        let Some(class_info) = ctx.module.class_infos.get(class_name) else {
            continue;
        };
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name)
            || answered.contains(&class_info.class_id)
            || class_info.properties.iter().any(|(name, _)| name == property)
            || !class_info.methods.contains_key(required_key)
        {
            continue;
        }
        let key = (
            class_info
                .method_impl_classes
                .get(required_key)
                .cloned()
                .unwrap_or_else(|| class_name.clone()),
            class_info.methods.contains_key(&isset_key).then(|| {
                class_info
                    .method_impl_classes
                    .get(&isset_key)
                    .cloned()
                    .unwrap_or_else(|| class_name.clone())
            }),
        );
        match groups.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, group)) => group.class_ids.push(class_info.class_id),
            None => groups.push((
                key,
                MixedMagicGetGroup {
                    class_ids: vec![class_info.class_id],
                    class_name: class_name.clone(),
                },
            )),
        }
    }
    let mut groups = groups.into_iter().map(|(_, group)| group).collect::<Vec<_>>();
    for group in &mut groups {
        group.class_ids.sort_unstable();
    }
    groups.sort_by_key(|group| group.class_ids[0]);
    Ok(groups)
}

/// Emits one `Mixed` ladder arm that answers a property read from the receiver's `__get`.
///
/// The unboxed receiver object is in the result register. A value read calls `__get(name)`; the
/// left operand of `??` first asks `__isset(name)` when the class declares it and answers null
/// when that is falsy, exactly as php's `BP_VAR_IS` fetch does. Any other mode, or a class
/// without the accessor php would call, answers null in silence. The arm leaves an owned boxed
/// `Mixed` in the result register and jumps to `done_label`.
fn emit_mixed_magic_get_arm(
    ctx: &mut FunctionContext<'_>,
    class_name: Option<&str>,
    property: &str,
    mode: PropertyFetchMode,
    done_label: &str,
) -> Result<()> {
    if magic_isset_alone(mode) {
        return emit_mixed_magic_isset_arm(ctx, class_name, property, done_label);
    }
    let wants_get = magic_calls_get(mode);
    let Some(class_name) = class_name.filter(|name| {
        wants_get
            && super::property_resolution::class_declares_method(ctx, name, "__get")
            // `empty()` on a class with `__get` but no `__isset` reports the name absent without
            // calling `__get`, as php does (`??` does call it).
            && (mode != PropertyFetchMode::Empty
                || super::property_resolution::class_declares_method(ctx, name, "__isset"))
    }) else {
        emit_boxed_null(ctx);
        abi::emit_jump(ctx.emitter, done_label);
        return Ok(());
    };
    let get_target = resolve_method_call_target(ctx, class_name, "__get", 2)?;
    if get_target.ref_params.first().copied().unwrap_or(false) {
        return Err(CodegenIrError::unsupported(format!(
            "magic __get by-reference name parameter on {}",
            class_name
        )));
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    if matches!(mode, PropertyFetchMode::Coalesce | PropertyFetchMode::Empty)
        && super::property_resolution::class_declares_method(ctx, class_name, "__isset")
    {
        let isset_target = resolve_method_call_target(ctx, class_name, "__isset", 2)?;
        let isset_ty = isset_target.return_ty.codegen_repr();
        let boxed_isset = matches!(isset_ty, PhpType::Mixed | PhpType::Union(_));
        if !(boxed_isset || matches!(isset_ty, PhpType::Bool | PhpType::Int))
            || isset_target.ref_params.first().copied().unwrap_or(false)
        {
            // An `__isset` this arm cannot read a truth value from keeps php's silent answer.
            emit_boxed_null(ctx);
            abi::emit_jump(ctx.emitter, done_label);
            return Ok(());
        }
        let label = ctx.next_label("mixed_prop_magic_isset_false");
        // Frame: receiver at 0, the boxed answer at 8, its truth value at 16. The receiver is
        // PINNED from `__isset` through `__get`, as php holds it for the whole fetch: an
        // `__isset` that drops the last other owner (`$root = null` on the global it came from)
        // otherwise freed the object `__get` was then called on.
        abi::emit_reserve_temporary_stack(ctx.emitter, 32);
        abi::emit_store_to_sp(ctx.emitter, result_reg, 0);
        abi::emit_call_label(ctx.emitter, "__rt_incref");
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        emit_magic_accessor_call(ctx, property, &isset_target)?;
        if boxed_isset {
            abi::emit_store_to_sp(ctx.emitter, result_reg, 8);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            abi::emit_store_to_sp(ctx.emitter, result_reg, 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 8);
            abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
            abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
        }
        abi::emit_branch_if_int_result_zero(ctx.emitter, &label);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        emit_magic_accessor_call(ctx, property, &get_target)?;
        box_method_result_as_mixed(ctx, &get_target.return_ty);
        // Unpin: the answer waits at 8 while the receiver's pin is released.
        abi::emit_store_to_sp(ctx.emitter, result_reg, 8);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Object(class_name.to_string()));
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 8);
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        abi::emit_jump(ctx.emitter, done_label);
        ctx.emitter.label(&label);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Object(class_name.to_string()));
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        emit_boxed_null(ctx);
        abi::emit_jump(ctx.emitter, done_label);
        return Ok(());
    }
    emit_magic_accessor_call(ctx, property, &get_target)?;
    box_method_result_as_mixed(ctx, &get_target.return_ty);
    abi::emit_jump(ctx.emitter, done_label);
    Ok(())
}

/// Emits the `isset()` (`Probe`) arm for a class php would ask: `__isset(name)` alone, answered as
/// a boxed `true` or `null` for the caller's `isset` test -- never `__get`, which php does not
/// call for `isset()`. A class without `__isset`, or one whose answer this arm cannot read as a
/// truth value, answers null as before.
///
/// `isset($mixed->private)` and `isset($mixed->undeclared)` answered false without asking: Twig's
/// `getAttribute()` probes `isset($object->$item)` on every object it renders, so an object that
/// exposes its data through `__isset`/`__get` printed nothing.
fn emit_mixed_magic_isset_arm(
    ctx: &mut FunctionContext<'_>,
    class_name: Option<&str>,
    property: &str,
    done_label: &str,
) -> Result<()> {
    let Some(class_name) = class_name
        .filter(|name| super::property_resolution::class_declares_method(ctx, name, "__isset"))
    else {
        emit_boxed_null(ctx);
        abi::emit_jump(ctx.emitter, done_label);
        return Ok(());
    };
    let isset_target = resolve_method_call_target(ctx, class_name, "__isset", 2)?;
    let isset_ty = isset_target.return_ty.codegen_repr();
    let boxed = matches!(isset_ty, PhpType::Mixed | PhpType::Union(_));
    if !(boxed || matches!(isset_ty, PhpType::Bool | PhpType::Int))
        || isset_target.ref_params.first().copied().unwrap_or(false)
    {
        emit_boxed_null(ctx);
        abi::emit_jump(ctx.emitter, done_label);
        return Ok(());
    }
    emit_magic_accessor_call(ctx, property, &isset_target)?;
    emit_isset_answer_as_boxed_flag(ctx, boxed, done_label);
    Ok(())
}

/// Turns the `__isset` answer in the result register into a boxed `true` (set) or `null` (not
/// set), releasing a boxed answer, and jumps to `done_label`.
fn emit_isset_answer_as_boxed_flag(ctx: &mut FunctionContext<'_>, boxed: bool, done_label: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    if boxed {
        abi::emit_reserve_temporary_stack(ctx.emitter, 16);
        abi::emit_store_to_sp(ctx.emitter, result_reg, 0);
        abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
        abi::emit_store_to_sp(ctx.emitter, result_reg, 8);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 8);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    }
    let unset_label = ctx.next_label("mixed_prop_magic_isset_probe_unset");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &unset_label);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 1);
    box_method_result_as_mixed(ctx, &PhpType::Bool);
    abi::emit_jump(ctx.emitter, done_label);
    ctx.emitter.label(&unset_label);
    emit_boxed_null(ctx);
    abi::emit_jump(ctx.emitter, done_label);
}

/// Classes whose `__get` a runtime-name property read must consult, with their class ids.
///
/// The shared by-name helper only knows the (class, property) pairs some class DECLARES; a class
/// that declares no `name` but has `__get` fell to the miss path and warned "Undefined property"
/// where php calls `__get('name')` -- Laravel's `Fluent::__get()`, behind every Blueprint
/// command (`$command->name`), so every schema operation and route that created a table failed.
pub(super) fn runtime_name_magic_get_classes(
    ctx: &FunctionContext<'_>,
    mode: PropertyFetchMode,
) -> Vec<(String, u64)> {
    // `isset()` asks `__isset` alone; a value read and `??` ask `__get`.
    let accessor = if magic_isset_alone(mode) { "__isset" } else { "__get" };
    let mut classes: Vec<(String, u64)> = ctx
        .module
        .class_infos
        .iter()
        .filter(|(_, info)| !info.is_abstract && info.methods.contains_key(&php_symbol_key(accessor)))
        .map(|(name, info)| (name.clone(), info.class_id))
        .collect();
    classes.sort_by_key(|(_, id)| *id);
    classes
}

/// One arm of the shared by-name read helper that answers through the receiver class's `__get`
/// (after `__isset` for `??`), with the property NAME read from the helper's stacked block.
///
/// The ladder frame holds the name pointer at 0, its length at 8 and the receiver object at 16;
/// the arm releases `frame_bytes` and leaves an owned boxed `Mixed` in the result register.
pub(super) fn emit_runtime_name_magic_get_arm(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    mode: PropertyFetchMode,
    frame_bytes: usize,
    done_label: &str,
) -> Result<()> {
    let target_arch = ctx.emitter.target;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let load_call_args = |ctx: &mut FunctionContext<'_>| {
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target_arch, 0), 16);
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target_arch, 1), 0);
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target_arch, 2), 8);
    };
    if magic_isset_alone(mode) {
        // `isset($o->$name)`: `__isset($name)` alone (see `emit_mixed_magic_isset_arm`).
        let isset_target = resolve_method_call_target(ctx, class_name, "__isset", 2)?;
        let isset_ty = isset_target.return_ty.codegen_repr();
        let boxed = matches!(isset_ty, PhpType::Mixed | PhpType::Union(_));
        if !(boxed || matches!(isset_ty, PhpType::Bool | PhpType::Int))
            || isset_target.ref_params.first().copied().unwrap_or(false)
        {
            abi::emit_release_temporary_stack(ctx.emitter, frame_bytes);
            emit_boxed_null(ctx);
            abi::emit_jump(ctx.emitter, done_label);
            return Ok(());
        }
        load_call_args(ctx);
        emit_resolved_method_call(ctx, &isset_target)?;
        abi::emit_release_temporary_stack(ctx.emitter, frame_bytes);
        emit_isset_answer_as_boxed_flag(ctx, boxed, done_label);
        return Ok(());
    }
    let get_target = resolve_method_call_target(ctx, class_name, "__get", 2)?;
    if get_target.ref_params.first().copied().unwrap_or(false) {
        abi::emit_release_temporary_stack(ctx.emitter, frame_bytes);
        emit_boxed_null(ctx);
        abi::emit_jump(ctx.emitter, done_label);
        return Ok(());
    }
    if mode == PropertyFetchMode::Empty
        && !super::property_resolution::class_declares_method(ctx, class_name, "__isset")
    {
        // `empty()` on a class with `__get` but no `__isset` reports the name absent.
        abi::emit_release_temporary_stack(ctx.emitter, frame_bytes);
        emit_boxed_null(ctx);
        abi::emit_jump(ctx.emitter, done_label);
        return Ok(());
    }
    if matches!(mode, PropertyFetchMode::Coalesce | PropertyFetchMode::Empty)
        && super::property_resolution::class_declares_method(ctx, class_name, "__isset")
    {
        let isset_target = resolve_method_call_target(ctx, class_name, "__isset", 2)?;
        let isset_ty = isset_target.return_ty.codegen_repr();
        let boxed_isset = matches!(isset_ty, PhpType::Mixed | PhpType::Union(_));
        if boxed_isset || matches!(isset_ty, PhpType::Bool | PhpType::Int) {
            let unset_label = ctx.next_label("mixed_dyn_prop_magic_unset");
            load_call_args(ctx);
            emit_resolved_method_call(ctx, &isset_target)?;
            if boxed_isset {
                abi::emit_reserve_temporary_stack(ctx.emitter, 16);
                abi::emit_store_to_sp(ctx.emitter, result_reg, 0);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
                abi::emit_store_to_sp(ctx.emitter, result_reg, 8);
                abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
                abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
                abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 8);
                abi::emit_release_temporary_stack(ctx.emitter, 16);
            }
            abi::emit_branch_if_int_result_zero(ctx.emitter, &unset_label);
            load_call_args(ctx);
            emit_resolved_method_call(ctx, &get_target)?;
            box_method_result_as_mixed(ctx, &get_target.return_ty);
            abi::emit_release_temporary_stack(ctx.emitter, frame_bytes);
            abi::emit_jump(ctx.emitter, done_label);
            ctx.emitter.label(&unset_label);
            abi::emit_release_temporary_stack(ctx.emitter, frame_bytes);
            emit_boxed_null(ctx);
            abi::emit_jump(ctx.emitter, done_label);
            return Ok(());
        }
    }
    load_call_args(ctx);
    emit_resolved_method_call(ctx, &get_target)?;
    box_method_result_as_mixed(ctx, &get_target.return_ty);
    abi::emit_release_temporary_stack(ctx.emitter, frame_bytes);
    abi::emit_jump(ctx.emitter, done_label);
    Ok(())
}

/// Calls a one-name magic accessor (`__get` / `__isset`) on the object in the result register.
fn emit_magic_accessor_call(
    ctx: &mut FunctionContext<'_>,
    property: &str,
    target: &super::super::MethodCallTarget,
) -> Result<()> {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    let target_arch = ctx.emitter.target;
    let this_reg = abi::int_arg_reg_name(target_arch, 0);
    let result_reg = abi::int_result_reg(ctx.emitter);
    if this_reg != result_reg {
        abi::emit_reg_move(ctx.emitter, this_reg, result_reg);
    }
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(target_arch, 1), &label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target_arch, 2), len as i64);
    emit_resolved_method_call(ctx, target)
}

/// Returns whether a frame can ask the eval bridge for an eval-owned object's property.
///
/// The owner probe finds the context that owns the object by itself, so a frame that holds an
/// eval context of its own needs it just as much: nothing else routes its runtime-name reads.
/// Requiring the frame to have NO context left `ReflectionProperty::getValue()` -- whose compiled
/// body reads `$object->{$name}` from a frame that does hold one -- answering null for every
/// private and protected property of an interpreted class.
pub(super) fn eval_owner_fallback_available(ctx: &FunctionContext<'_>) -> bool {
    ctx.module.required_runtime_features.eval_bridge
}

/// Probes a declared typed slot through a receiver whose class is only known at run time.
///
/// `isset()`, `empty()` and `??` all ask this question before reading, precisely so they never
/// raise on an uninitialized typed property. Until this path existed the question could only be
/// asked of a receiver whose class the compiler knew, so an UNTYPED parameter — `function
/// f($container) { return isset($container->name); }`, and every generated Symfony container
/// factory is written that way — fell back to the ordinary read and died with
/// "Typed property C::$name must not be accessed before initialization" where PHP answers false.
///
/// Only the declared-slot branches answer for themselves. A stdClass receiver, a class that does
/// not declare the property at all, and every other miss answer "initialized", which sends the
/// caller to the ordinary read it has always taken — magic `__isset`, dynamic properties and the
/// undefined-property warning all keep behaving exactly as before. A receiver that is not an
/// object answers false, which is what PHP says for `isset($notAnObject->p)`.
pub(super) fn lower_mixed_prop_initialized(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    let candidates = declared_mixed_property_slot_candidates(ctx, property, inst)?;
    let not_object_label = ctx.next_label("mixed_prop_init_not_object");
    let fallthrough_label = ctx.next_label("mixed_prop_init_fallthrough");
    let stdclass_label = ctx.next_label("mixed_prop_init_stdclass");
    let done_label = ctx.next_label("mixed_prop_init_done");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_prop_init_{}",
                label_fragment(&candidate.slot.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_mixed_object_payload_or_null(ctx, &not_object_label);
    let class_ids = candidates.iter().map(|candidate| candidate.class_id).collect::<Vec<_>>();
    emit_mixed_property_class_dispatch(
        ctx,
        &class_ids,
        &match_labels,
        &stdclass_label,
        &fallthrough_label,
    );

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let base_reg = abi::int_result_reg(ctx.emitter);
        if candidate.slot.is_declared {
            emit_typed_property_initialized_bool(ctx, &candidate.slot, base_reg);
        } else {
            // An untyped slot is plain null from construction, so it is always initialized.
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        }
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&stdclass_label);
    ctx.emitter.label(&fallthrough_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&not_object_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);

    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers a `Mixed` receiver through the runtime stdClass-style property helper.
pub(super) fn lower_runtime_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    let done_label = ctx.next_label("mixed_prop_runtime_done");
    if eval_owner_fallback_available(ctx) {
        let runtime_label = ctx.next_label("mixed_prop_runtime");
        builtins::lower_eval_owner_mixed_property_get(
            ctx,
            object,
            property,
            &runtime_label,
            &done_label,
        )?;
        ctx.emitter.label(&runtime_label);
    }
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_property_get");
    ctx.emitter.label(&done_label);
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// One class-only arm of a `Mixed` ladder: the runtime class answers this name from its OWN
/// per-instance hash, at its own offset, and no declared slot is involved.
///
/// This is the single mechanism every `Mixed` ladder uses for that case, for a literal name and
/// for a runtime name alike. Without it a ladder matched only DECLARED names and then fell into a
/// miss path that understands `stdClass` alone, so an `#[\AllowDynamicProperties]` user class
/// could not read back what it had just stored: the write went to the class's hash and the read
/// answered `null`, or the write was dropped outright.
/// Splits off the read hash arms the `_class_dyn_hash_rows` table answers exactly.
///
/// Same rule as the write side (`split_table_routed_dynamic_hash_candidates`): the class keeps
/// the name only in its hash (it declares no slot under it, so no scope rule can turn it into a
/// mangled private) and its arm's offset is the one the table records. Returns
/// `(explicit arms, table-routed class ids)`.
fn split_table_routed_read_hash_arms(
    ctx: &FunctionContext<'_>,
    arms: Vec<MixedClassHashArm>,
    property: &str,
) -> (Vec<MixedClassHashArm>, Vec<u64>) {
    let mut explicit = Vec::new();
    let mut routed = Vec::new();
    for arm in arms {
        let table_answers = ctx.module.class_infos.get(&arm.class_name).is_some_and(|info| {
            info.dynamic_property_hash_is_name_addressable()
                && !info.properties.iter().any(|(declared, _)| declared == property)
                && arm.hash_offset
                    == super::allocation_clone::dynamic_property_hash_offset(info.properties.len())
        });
        if table_answers {
            routed.push(arm.class_id);
        } else {
            explicit.push(arm);
        }
    }
    (explicit, routed)
}

/// Classes the row table would answer that this read must NOT answer from their hash: they have
/// a row but are neither table-routed nor matched by an explicit arm earlier in the ladder.
fn table_addressable_classes_not_routed(
    ctx: &FunctionContext<'_>,
    routed: &[u64],
    dispatched: &[u64],
) -> Vec<u64> {
    let mut ids = ctx
        .module
        .class_infos
        .iter()
        .filter(|(name, info)| {
            info.dynamic_property_hash_is_name_addressable()
                && !crate::types::checker::builtin_stdclass::is_stdclass(name)
                && !routed.contains(&info.class_id)
                && !dispatched.contains(&info.class_id)
        })
        .map(|(_, info)| info.class_id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

/// Reads a literal-name dynamic property through the receiver class's `_class_dyn_hash_rows`
/// row and jumps to `done_label` with the owned boxed entry when the hash holds it.
///
/// Expects the unboxed receiver in the result register and leaves it there when it falls
/// through: a class without a row, one listed in `outside`, or a hash without the name continue
/// to the miss code that follows, which is where those receivers always went.
fn emit_table_dynamic_hash_read(
    ctx: &mut FunctionContext<'_>,
    property: &str,
    outside: &[u64],
    done_label: &str,
) {
    let skip_label = ctx.next_label("table_dyn_prop_read_skip");
    let (key_label, key_len) = ctx.data.add_string(property.as_bytes());
    let target = ctx.emitter.target;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the receiver's runtime class id
            for class_id in outside {
                abi::emit_load_int_immediate(ctx.emitter, "x10", *class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // a row this read must not answer from
                ctx.emitter.instruction(&format!("b.eq {}", skip_label));       // leave it to the miss path
            }
            abi::emit_symbol_address(ctx.emitter, "x11", "_class_name_count");
            ctx.emitter.instruction("ldr x11, [x11]");                          // load the dense class table bound
            ctx.emitter.instruction("cmp x9, x11");                             // ids outside the table have no row
            ctx.emitter.instruction(&format!("b.hs {}", skip_label));           // leave them to the miss path
            abi::emit_symbol_address(ctx.emitter, "x11", "_class_dyn_hash_rows");
            ctx.emitter.instruction("add x11, x11, x9, lsl #4");                // address the class's 16-byte row
            ctx.emitter.instruction("ldr x12, [x11]");                          // load the per-instance hash offset
            ctx.emitter.instruction(&format!("cbz x12, {}", skip_label));       // no addressable hash: the miss path answers
            ctx.emitter.instruction("ldr x0, [x0, x12]");                       // pass the receiver's dynamic-property hash
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r11, QWORD PTR [rax]");                // load the receiver's runtime class id
            for class_id in outside {
                abi::emit_load_int_immediate(ctx.emitter, "r10", *class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // a row this read must not answer from
                ctx.emitter.instruction(&format!("je {}", skip_label));         // leave it to the miss path
            }
            ctx.emitter.instruction("cmp r11, QWORD PTR [rip + _class_name_count]"); // ids outside the table have no row
            ctx.emitter.instruction(&format!("jae {}", skip_label));            // leave them to the miss path
            ctx.emitter.instruction("lea r10, [rip + _class_dyn_hash_rows]");   // materialize the row table base
            ctx.emitter.instruction("shl r11, 4");                              // scale the class id to its 16-byte row
            ctx.emitter.instruction("mov r10, QWORD PTR [r10 + r11]");          // load the per-instance hash offset
            ctx.emitter.instruction("test r10, r10");                           // does this class have an addressable hash?
            ctx.emitter.instruction(&format!("jz {}", skip_label));             // no: the miss path answers
            ctx.emitter.instruction("mov rdi, QWORD PTR [rax + r10]");          // pass the receiver's dynamic-property hash
        }
    }
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(target, 1), &key_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 2), key_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &skip_label);
    match target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // return the boxed Mixed cell stored in the hash entry
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // return the boxed Mixed cell stored in the hash entry
        }
    }
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_jump(ctx.emitter, done_label);
    ctx.emitter.label(&skip_label);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
}

pub(super) struct MixedClassHashArm {
    /// Runtime class id the arm matches on.
    pub(super) class_id: u64,
    /// That class's name, which php reports in its creation notice and its warning.
    pub(super) class_name: String,
    /// That class's own `8 + slots * 16` hash offset.
    pub(super) hash_offset: usize,
}

/// Collects the classes that answer `property` from their own hash and are NOT already an arm.
///
/// `declared_class_ids` are the classes the caller's declared-name ladder already covers, so a
/// class stays with the answer its declared arm gives it and is never probed twice. `stdClass` is
/// excluded because every ladder probes it separately, through its own runtime helper.
/// Drops, for a value read, the hash arm of a class that declares `__get`, so the read reaches
/// the magic arm instead.
///
/// A hash arm answers only from the per-instance property table and warns on a miss; php asks
/// `__get` there. Every compiled class with a dynamic-property table got such an arm, so Laravel's
/// `Fluent` (`$command->name` through `__get`) read null with "Undefined property" and every
/// schema operation failed. A dynamic property stored on a `__get` class is the one case this
/// gives up, and php code that has `__get` keeps its data behind it.
fn read_hash_arms_without_magic(
    ctx: &FunctionContext<'_>,
    arms: Vec<MixedClassHashArm>,
    mode: PropertyFetchMode,
) -> Vec<MixedClassHashArm> {
    if !magic_calls_get(mode) {
        return arms;
    }
    let get_key = php_symbol_key("__get");
    arms.into_iter()
        .filter(|arm| {
            !ctx.module
                .class_infos
                .get(&arm.class_name)
                .is_some_and(|info| info.methods.contains_key(&get_key))
        })
        .collect()
}

pub(super) fn mixed_class_hash_arms(
    ctx: &FunctionContext<'_>,
    property: &str,
    declared_class_ids: &[u64],
) -> Result<Vec<MixedClassHashArm>> {
    let mut arms = Vec::new();
    // `dynamic_property_hash_offset_for_class` answers `None` for every class whose hash is not
    // name-addressable, so only those few are asked (see `Module::name_addressable_hash_classes`).
    let addressable = ctx.module.name_addressable_hash_classes();
    for class_name in addressable.iter() {
        let Some(class_info) = ctx.module.class_infos.get(class_name) else {
            continue;
        };
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name)
            || declared_class_ids.contains(&class_info.class_id)
        {
            continue;
        }
        let Some(hash_offset) = dynamic_property_hash_offset_for_class(ctx, class_name, property)?
        else {
            continue;
        };
        arms.push(MixedClassHashArm {
            class_id: class_info.class_id,
            class_name: class_name.clone(),
            hash_offset,
        });
    }
    arms.sort_by_key(|arm| arm.class_id);
    Ok(arms)
}

/// One runtime-class arm of a `Mixed` receiver's reference-cell load.
///
/// A class this scope may not reach keeps its arm, because the ladder dispatches on the runtime
/// class id and a class that is DROPPED falls into the shared no-cell path instead. That path
/// used to publish a zero pointer as a live alias, which every later load or store through the
/// alias would dereference, so an omitted class was strictly more dangerous than a refused one.
pub(super) struct MixedReferenceCandidate {
    /// Class id and the physical slot the ladder matches on.
    pub(super) candidate: MixedPropertyCandidate,
    /// php's catchable access-error message when this scope may not reach the name at all.
    pub(super) refusal: Option<String>,
}

/// Collects the reference-cell arms a `Mixed` receiver's property can take, by runtime class.
///
/// A class whose answer is `Dynamic` makes the WHOLE lowering an explicit `unsupported`
/// diagnostic rather than an arm: php binds the reference to a distinct dynamic property in the
/// per-instance hash, which is a storage shape this backend cannot alias yet for any class, and a
/// ladder that quietly omitted such a class would send it to the no-cell path instead.
pub(super) fn declared_mixed_reference_property_candidates(
    ctx: &FunctionContext<'_>,
    property: &str,
    inst: &Instruction,
) -> Result<Vec<MixedReferenceCandidate>> {
    let mut candidates = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
            continue;
        }
        if !class_info
            .properties
            .iter()
            .any(|(name, _)| name == property)
        {
            continue;
        }
        let refusal = match resolve_property_reference_arm(ctx, class_name, property, inst)? {
            Some(PropertyNameArm::Slot(slot)) => {
                if !slot.is_reference {
                    continue;
                }
                candidates.push(MixedReferenceCandidate {
                    candidate: MixedPropertyCandidate {
                        class_id: class_info.class_id,
                        slot,
                    },
                    refusal: None,
                });
                continue;
            }
            Some(PropertyNameArm::Refuse { message, .. }) => message,
            // A class whose `$property` resolves to the per-instance hash (a parent's PRIVATE
            // `$instanceof` seen from `ChildDefinition`) gets no arm, so a receiver of that class
            // reaches the no-cell error at run time. Refusing the whole function instead refused
            // every Symfony closure binding `&$this->instanceof` on an unrelated loader class.
            _ => continue,
        };
        // A refusal arm still needs a slot to dispatch on. It is never loaded: the arm raises.
        let Ok(slot) = resolve_property_slot_for_class(ctx, class_name, property, inst) else {
            continue;
        };
        candidates.push(MixedReferenceCandidate {
            candidate: MixedPropertyCandidate {
                class_id: class_info.class_id,
                slot,
            },
            refusal: Some(refusal),
        });
    }
    candidates.sort_by_key(|candidate| candidate.candidate.class_id);
    Ok(candidates)
}

/// Collects declared-property candidates for a property read on an unknown `Mixed` object.
pub(super) fn declared_mixed_property_candidates(
    ctx: &FunctionContext<'_>,
    property: &str,
    mode: PropertyFetchMode,
    inst: &Instruction,
) -> Result<Vec<MixedPropertyReadCandidate>> {
    let mut candidates = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
            continue;
        }
        let backing = super::property_resolution::reflection_backing(ctx, class_name, property)
        .unwrap_or(property);
        if !class_info
            .properties
            .iter()
            .any(|(name, _)| name == backing)
        {
            continue;
        }
        // A strict ancestor's private slot is not addressable by this name from anywhere but the
        // class that declared it, so this receiver class is not a candidate for the read and the
        // name falls into the miss arm, which is php's dynamic-property answer.
        //
        // A name php REFUSES is a different answer again: the arm exists so the class id still
        // dispatches here, but it raises instead of reading. Without it a boxed receiver was the
        // one remaining way to read private storage from an unrelated scope by plain name.
        let arm = resolve_property_read_arm(ctx, class_name, property, mode, inst)?;
        let (slot, kind) = match arm {
            Some(PropertyNameArm::Slot(slot)) => (slot, MixedPropertyReadKind::Slot),
            Some(PropertyNameArm::Refuse { message, .. }) => {
                let Ok(slot) = resolve_property_slot_for_class(ctx, class_name, property, inst)
                else {
                    continue;
                };
                (slot, MixedPropertyReadKind::Refuse(message))
            }
            // php answers the accessor here, so the arm must exist and must NOT read the slot.
            // Dropping it would send the name to the miss arm, which warns for a value read.
            Some(PropertyNameArm::MagicDeferred) => {
                let Ok(slot) = resolve_property_slot_for_class(ctx, class_name, property, inst)
                else {
                    continue;
                };
                (slot, MixedPropertyReadKind::MagicDeferred)
            }
            // A literal name php resolves to a DYNAMIC property answers from THIS class's
            // per-instance hash, so the arm exists whenever that class reserves one. Its own
            // miss still warns for a value read, exactly like the shared miss arm, so a class
            // with no hash keeps taking that shorter route and emits the code it emitted before.
            Some(PropertyNameArm::ScopeDynamic)
                if dynamic_property_hash_offset_for_class(ctx, class_name, property)?.is_some() =>
            {
                let Ok(slot) = resolve_property_slot_for_class(ctx, class_name, property, inst)
                else {
                    continue;
                };
                (slot, MixedPropertyReadKind::ScopeDynamic)
            }
            _ => continue,
        };
        candidates.push(MixedPropertyReadCandidate {
            candidate: MixedPropertyCandidate {
                class_id: class_info.class_id,
                slot,
            },
            kind,
        });
    }
    candidates.sort_by_key(|candidate| candidate.candidate.class_id);
    Ok(candidates)
}

/// Promotes an unboxed Mixed object payload into the normal result register or jumps to null.
pub(super) fn emit_mixed_object_payload_or_null(ctx: &mut FunctionContext<'_>, null_label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // check whether the Mixed receiver holds an object payload
            ctx.emitter.instruction(&format!("b.ne {}", null_label));           // non-object Mixed receivers produce a null property result
            ctx.emitter.instruction("mov x0, x1");                              // promote the unboxed object payload for class-id dispatch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // check whether the Mixed receiver holds an object payload
            ctx.emitter.instruction(&format!("jne {}", null_label));            // non-object Mixed receivers produce a null property result
            ctx.emitter.instruction("mov rax, rdi");                            // promote the unboxed object payload for class-id dispatch
        }
    }
}

/// Emits class-id dispatch for declared property candidates, stdClass, and a real miss branch.
///
/// The arms are identified by CLASS ID alone. A slot is not required and deliberately not taken:
/// an arm can answer from the runtime class's per-instance hash, which has no declared slot to
/// name, and requiring one is what kept those arms out of the ladder in the first place.
///
/// Consecutive ids bound for one label take a single range test: the ~450 classes Symfony's dumped
/// container derives from itself have consecutive ids and share every inherited property's arm.
pub(super) fn emit_mixed_property_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    class_ids: &[u64],
    match_labels: &[String],
    stdclass_label: &str,
    miss_label: &str,
) {
    let tests = class_id_range_tests(class_ids, match_labels);
    let (class_id_reg, scratch_reg, branch_equal, branch_below, branch_not_above) =
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x9, [x0]");                        // load the receiver class id for Mixed property dispatch
                ("x9", "x10", "b.eq", "b.lt", "b.le")
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov r11, QWORD PTR [rax]");            // load the receiver class id for Mixed property dispatch
                ("r11", "r10", "je", "jl", "jle")
            }
        };
    for (first, last, label) in tests {
        abi::emit_load_int_immediate(ctx.emitter, scratch_reg, first as i64);
        ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, scratch_reg)); // compare the receiver class id against this declared-property owner
        if first == last {
            ctx.emitter.instruction(&format!("{} {}", branch_equal, label));    // read the declared property when the class id matches
            continue;
        }
        let next = ctx.next_label("mixed_prop_class_range_next");
        ctx.emitter.instruction(&format!("{} {}", branch_below, next));         // an id below the range cannot match it
        abi::emit_load_int_immediate(ctx.emitter, scratch_reg, last as i64);
        ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, scratch_reg)); // compare the receiver class id against the range's last id
        ctx.emitter.instruction(&format!("{} {}", branch_not_above, label));    // read the declared property when the class id lies in the range
        ctx.emitter.label(&next);
    }
    emit_branch_to_stdclass_candidate(ctx, class_id_reg, scratch_reg, stdclass_label);
    abi::emit_jump(ctx.emitter, miss_label);
}

/// The inclusive `(first, last, label)` id ranges of a dispatch, sorted by id.
///
/// An id listed twice keeps its first label, as a ladder testing them in order would. A run of
/// two stays two tests: a range test is no shorter.
fn class_id_range_tests<'a>(class_ids: &[u64], labels: &'a [String]) -> Vec<(u64, u64, &'a str)> {
    let mut ordered: Vec<(u64, &'a str)> = class_ids
        .iter()
        .zip(labels.iter())
        .map(|(id, label)| (*id, label.as_str()))
        .collect();
    ordered.sort_by_key(|(id, _)| *id);
    ordered.dedup_by_key(|(id, _)| *id);
    let mut ranges: Vec<(u64, u64, &'a str)> = Vec::new();
    for (id, label) in ordered {
        match ranges.last_mut() {
            Some((_, last, run_label)) if *last + 1 == id && *run_label == label => *last = id,
            _ => ranges.push((id, id, label)),
        }
    }
    let mut tests = Vec::with_capacity(ranges.len());
    for (first, last, label) in ranges {
        if last == first + 1 {
            tests.push((first, first, label));
            tests.push((last, last, label));
        } else {
            tests.push((first, last, label));
        }
    }
    tests
}

#[cfg(test)]
mod class_id_range_tests_tests {
    use super::class_id_range_tests;

    fn labels(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn consecutive_ids_of_one_label_become_one_range() {
        let names = labels(&["a", "a", "a", "a"]);
        assert_eq!(class_id_range_tests(&[7, 5, 6, 8], &names), vec![(5, 8, "a")]);
    }

    #[test]
    fn a_gap_or_another_label_ends_a_range() {
        let names = labels(&["a", "a", "a", "b", "b", "b", "a", "a", "a"]);
        assert_eq!(
            class_id_range_tests(&[1, 2, 3, 4, 5, 6, 8, 9, 10], &names),
            vec![(1, 3, "a"), (4, 6, "b"), (8, 10, "a")]
        );
    }

    #[test]
    fn a_pair_stays_two_tests_and_a_repeated_id_keeps_its_first_label() {
        let names = labels(&["a", "a", "b"]);
        assert_eq!(
            class_id_range_tests(&[1, 2, 1], &names),
            vec![(1, 1, "a"), (2, 2, "a")]
        );
    }
}

/// Branches to the stdClass fallback when the runtime module contains stdClass metadata.
pub(super) fn emit_branch_to_stdclass_candidate(
    ctx: &mut FunctionContext<'_>,
    class_id_reg: &str,
    scratch_reg: &str,
    stdclass_label: &str,
) {
    let Some(stdclass_id) = stdclass_class_id(ctx) else {
        return;
    };
    abi::emit_load_int_immediate(ctx.emitter, scratch_reg, stdclass_id as i64);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", class_id_reg, scratch_reg)); // check whether the object uses stdClass dynamic storage
            ctx.emitter.instruction(&format!("b.eq {}", stdclass_label));       // route stdClass reads through the hash-backed helper
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", class_id_reg, scratch_reg)); // check whether the object uses stdClass dynamic storage
            ctx.emitter.instruction(&format!("je {}", stdclass_label));         // route stdClass reads through the hash-backed helper
        }
    }
}

/// Returns the runtime class id assigned to stdClass in this module.
pub(super) fn stdclass_class_id(ctx: &FunctionContext<'_>) -> Option<u64> {
    ctx.module
        .class_infos
        .iter()
        .find(|(class_name, _)| crate::types::checker::builtin_stdclass::is_stdclass(class_name))
        .map(|(_, class_info)| class_info.class_id)
}

/// Boxes or retains a declared-property load so Mixed receiver paths produce owned Mixed cells.
pub(super) fn box_mixed_property_candidate_result(ctx: &mut FunctionContext<'_>, source_ty: &PhpType) {
    // A `?ptr<T>` slot keeps its union, so address 0 is PHP null.
    let box_ty = crate::types::mixed_box_php_type(source_ty);
    let source_ty = source_ty.codegen_repr();
    if source_ty == PhpType::Mixed {
        abi::emit_incref_if_refcounted(ctx.emitter, &source_ty);
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
    }
}

/// Reads a static property name from an already-unboxed stdClass payload.
pub(super) fn emit_stdclass_get_from_loaded_object(ctx: &mut FunctionContext<'_>, property: &str) {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the unboxed stdClass object pointer to the dynamic getter
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
        }
    }
}

/// Branches when `__rt_mixed_unbox` returned an object payload tag.
pub(super) fn emit_branch_if_mixed_unboxed_object(ctx: &mut FunctionContext<'_>, object_label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the boxed union holds an object payload
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // read the declared property only for object payloads
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the boxed union holds an object payload
            ctx.emitter.instruction(&format!("je {}", object_label));           // read the declared property only for object payloads
        }
    }
}

/// Moves the low payload produced by `__rt_mixed_unbox` into the object base register.
pub(super) fn move_mixed_unboxed_object_payload(ctx: &mut FunctionContext<'_>, base_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, x1", base_reg));          // use the unboxed object pointer as the declared-property base
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, rdi", base_reg));         // use the unboxed object pointer as the declared-property base
        }
    }
}

/// Lowers `$maybeObject->property`, warning when the receiver is PHP null.
pub(super) fn lower_nullable_prop_get_with_warning(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    if class_name.trim_start_matches('\\').is_empty() {
        return lower_nullable_generic_object_prop_get(ctx, inst, object, property);
    }
    let null_label = ctx.next_label("nullable_prop_warning_null");
    let done_label = ctx.next_label("nullable_prop_warning_done");
    // A slot this scope may not read, on a class php asks instead (`__get`, or `__isset` for a
    // probe): `?R $r; $r->private` read the private value -- or died on an uninitialized one --
    // where php calls `R::__get`. The non-nullable receiver already took that answer.
    let mode = property_fetch_mode(inst);
    if matches!(
        super::property_resolution::resolve_property_read_arm(ctx, class_name, property, mode, inst)?,
        Some(PropertyNameArm::MagicDeferred)
    ) {
        let object_reg = abi::int_result_reg(ctx.emitter);
        emit_nullable_receiver_object_payload(ctx, object, &null_label, object_reg)?;
        emit_mixed_magic_get_arm(ctx, Some(class_name), property, mode, &done_label)?;
        ctx.emitter.label(&null_label);
        if inst.op != Op::NullsafePropGet {
            emit_property_on_null_warning(ctx, property);
        }
        emit_boxed_null(ctx);
        ctx.emitter.label(&done_label);
        return store_if_result(ctx, inst);
    }
    let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
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
    if inst.op != Op::NullsafePropGet {
        emit_property_on_null_warning(ctx, property);
    }
    emit_boxed_null(ctx);

    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers a nullable bare-object property read through concrete runtime class dispatch.
fn lower_nullable_generic_object_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    let null_label = ctx.next_label("nullable_generic_prop_null");
    let done_label = ctx.next_label("nullable_generic_prop_done");
    let object_reg = abi::int_result_reg(ctx.emitter);
    emit_nullable_receiver_object_payload(ctx, object, &null_label, object_reg)?;
    lower_polymorphic_object_prop_get_from_loaded_object(ctx, inst, "", property)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    if inst.op != Op::NullsafePropGet {
        emit_property_on_null_warning(ctx, property);
    }
    emit_boxed_null(ctx);
    store_if_result(ctx, inst)?;

    ctx.emitter.label(&done_label);
    Ok(())
}


/// Emits PHP's warning for reading a property from null.
pub(super) fn emit_property_on_null_warning(ctx: &mut FunctionContext<'_>, property: &str) {
    let message = format!(
        "Warning: Attempt to read property \"{}\" on null\n",
        property
    );
    emit_static_property_warning(ctx, &message);
}

/// Emits `Warning: Undefined property: Class::$name` when both names are compile-time constants.
///
/// The runtime-class sibling below exists for a receiver whose class is only known at run time.
/// A scope-resolved dynamic name has both halves statically, so it needs no fragment assembly.
pub(super) fn emit_undefined_property_warning(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
) {
    let message = format!(
        "Warning: Undefined property: {}::${}\n",
        class_name.trim_start_matches('\\'),
        property
    );
    emit_static_property_warning(ctx, &message);
}

/// Writes one fully formed warning line through the suppressible PHP warning channel.
fn emit_static_property_warning(ctx: &mut FunctionContext<'_>, message: &str) {
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the warning line byte length
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &message_label);
            ctx.emitter
                .instruction(&format!("mov esi, {}", message_len)); // pass the warning line byte length
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}

/// Emits `Warning: Undefined property: Class::$name` for an object already in the result register.
pub(super) fn emit_undefined_property_warning_for_loaded_object(
    ctx: &mut FunctionContext<'_>,
    property: &str,
) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the missing-property receiver class id
            abi::emit_symbol_address(ctx.emitter, "x10", "_class_name_entries");
            ctx.emitter.instruction("lsl x11, x9, #4");                         // scale the class id to the 16-byte class-name row
            ctx.emitter.instruction("add x10, x10, x11");                       // address the receiver's class-name metadata
            ctx.emitter.instruction("ldr x1, [x10]");                           // load the receiver class-name pointer
            ctx.emitter.instruction("ldr x2, [x10, #8]");                       // load the receiver class-name byte length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // load the missing-property receiver class id
            abi::emit_symbol_address(ctx.emitter, "r10", "_class_name_entries");
            ctx.emitter.instruction("shl r9, 4");                               // scale the class id to the 16-byte class-name row
            ctx.emitter.instruction("mov rax, QWORD PTR [r10 + r9]");           // load the receiver class-name pointer
            ctx.emitter.instruction("mov rdx, QWORD PTR [r10 + r9 + 8]");       // load the receiver class-name byte length
        }
    }
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_property_warning_fragment(ctx, b"Warning: Undefined property: ", false);
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2"),
        Arch::X86_64 => abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi"),
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    emit_property_warning_fragment(ctx, format!("::${}\n", property).as_bytes(), true);
}

/// Collects every class's declared slot for `property`, for the write and initialized-probe paths.
///
/// Unlike [`declared_mixed_property_candidates`], which models php's scope-dependent READ answers
/// (refusal, deferred magic, scope-dynamic), a write or an `isset`-style probe on a declared slot
/// addresses the slot itself, so each candidate is just the class id and the slot.
pub(super) fn declared_mixed_property_slot_candidates(
    ctx: &FunctionContext<'_>,
    property: &str,
    inst: &Instruction,
) -> Result<Vec<MixedPropertyCandidate>> {
    let mut candidates = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
            continue;
        }
        let backing = super::property_resolution::reflection_backing(ctx, class_name, property)
        .unwrap_or(property);
        if !class_info
            .properties
            .iter()
            .any(|(name, _)| name == backing)
        {
            continue;
        }
        let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
        candidates.push(MixedPropertyCandidate {
            class_id: class_info.class_id,
            slot,
        });
    }
    candidates.sort_by_key(|candidate| candidate.class_id);
    Ok(candidates)
}
