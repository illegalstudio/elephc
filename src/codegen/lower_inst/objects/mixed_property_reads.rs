//! Purpose:
//! Lowers property reads from boxed Mixed and union receivers.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Runtime class dispatch produces owned Mixed results and preserves null warnings.

use super::*;

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
    let mode = property_fetch_mode(inst);
    let candidates = declared_mixed_property_candidates(ctx, property, mode, inst)?;
    // A user class may answer this name exclusively from its per-instance property hash. Such
    // a class contributes no declared candidate, but still needs the class-id ladder below. A
    // direct fallback to the stdClass-shaped helper would lose that class altogether.
    let has_hash_arms = !mixed_class_hash_arms(ctx, property, &[])?.is_empty();
    if !candidates.is_empty() || has_hash_arms {
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
    let mut dispatch = candidates
        .iter()
        .map(|candidate| candidate.candidate.class_id)
        .collect::<Vec<_>>();
    // A class that keeps this name in its own per-instance hash gets an arm too, even though it
    // declares no slot for it. Without one an `#[\AllowDynamicProperties]` user class fell into
    // the miss path, which understands `stdClass` alone, and could not read back what a write
    // through the same Mixed receiver had just stored in its hash.
    let hash_arms = mixed_class_hash_arms(ctx, property, &dispatch)?;
    for arm in &hash_arms {
        match_labels.push(ctx.next_label(&format!(
            "mixed_prop_hash_{}",
            label_fragment(&arm.class_name)
        )));
        dispatch.push(arm.class_id);
    }

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_mixed_object_payload_or_null(ctx, &null_label);
    emit_mixed_property_class_dispatch(
        ctx,
        &dispatch,
        &match_labels,
        &stdclass_label,
        &miss_label,
    );

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        // Only `Slot` touches storage. php refuses the read from this scope, or answers an
        // accessor this compiler cannot dispatch for a runtime name yet; either way the private
        // slot stays unread. Only a value read raises: the probes answer null in silence.
        match &candidate.kind {
            MixedPropertyReadKind::Refuse(message) => {
                super::super::exceptions::emit_error(ctx, message);
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
            // php answers `__get` or `__isset`, which a runtime name cannot reach yet.
            MixedPropertyReadKind::MagicDeferred => {
                emit_boxed_null(ctx);
                abi::emit_jump(ctx.emitter, &done_label);
                continue;
            }
            MixedPropertyReadKind::Slot => {}
        }
        let slot = &candidate.candidate.slot;
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

    ctx.emitter.label(&stdclass_label);
    emit_stdclass_get_from_loaded_object(ctx, property);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
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

/// Lowers a `Mixed` receiver through the runtime stdClass-style property helper.
pub(super) fn lower_runtime_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
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
pub(super) fn mixed_class_hash_arms(
    ctx: &FunctionContext<'_>,
    property: &str,
    declared_class_ids: &[u64],
) -> Result<Vec<MixedClassHashArm>> {
    let mut arms = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
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
            _ => {
                return Err(CodegenIrError::unsupported(format!(
                    "{} for a Mixed receiver whose class {} resolves ${} to a dynamic property; \
                     binding a reference into the per-instance property hash is not supported",
                    inst.op.name(),
                    class_name,
                    property
                )))
            }
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
        if !class_info
            .properties
            .iter()
            .any(|(name, _)| name == property)
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
pub(super) fn emit_mixed_property_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    class_ids: &[u64],
    match_labels: &[String],
    stdclass_label: &str,
    miss_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the receiver class id for Mixed property dispatch
            for (class_id, label) in class_ids.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", *class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare the receiver class id against this declared-property owner
                ctx.emitter.instruction(&format!("b.eq {}", label));            // read the declared property when the class id matches
            }
            emit_branch_to_stdclass_candidate(ctx, "x9", "x10", stdclass_label);
            abi::emit_jump(ctx.emitter, miss_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r11, QWORD PTR [rax]");                // load the receiver class id for Mixed property dispatch
            for (class_id, label) in class_ids.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", *class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare the receiver class id against this declared-property owner
                ctx.emitter.instruction(&format!("je {}", label));              // read the declared property when the class id matches
            }
            emit_branch_to_stdclass_candidate(ctx, "r11", "r10", stdclass_label);
            abi::emit_jump(ctx.emitter, miss_label);
        }
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
    let source_ty = source_ty.codegen_repr();
    if source_ty == PhpType::Mixed {
        abi::emit_incref_if_refcounted(ctx.emitter, &source_ty);
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &source_ty);
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
    let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
    let null_label = ctx.next_label("nullable_prop_warning_null");
    let done_label = ctx.next_label("nullable_prop_warning_done");
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
    emit_property_on_null_warning(ctx, property);
    emit_boxed_null(ctx);

    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
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
