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
    if class_property_is_missing(ctx, class_name, property)? {
        return lower_union_undefined_property_get(ctx, inst, object, property);
    }
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
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    emit_property_load(ctx, &slot, base_reg)?;
    materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
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
    let candidates = declared_mixed_property_candidates(ctx, property, inst)?;
    if !candidates.is_empty() {
        return lower_declared_mixed_prop_get(ctx, inst, object, property, candidates);
    }
    lower_runtime_mixed_prop_get(ctx, inst, object, property)
}

/// Lowers a `Mixed` receiver by dispatching known user classes before stdClass fallback.
pub(super) fn lower_declared_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
    candidates: Vec<MixedPropertyCandidate>,
) -> Result<()> {
    let null_label = ctx.next_label("mixed_prop_null");
    let miss_label = ctx.next_label("mixed_prop_miss");
    let done_label = ctx.next_label("mixed_prop_done");
    let stdclass_label = ctx.next_label("mixed_prop_stdclass");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_prop_{}",
                label_fragment(&candidate.slot.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_mixed_object_payload_or_null(ctx, &null_label);
    emit_mixed_property_class_dispatch(
        ctx,
        &candidates,
        &match_labels,
        &stdclass_label,
        &miss_label,
    );

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let base_reg = abi::int_result_reg(ctx.emitter);
        if let Some(target) =
            property_hook_get_target(ctx, &candidate.slot.class_name, property)?
        {
            emit_property_hook_get_result(ctx, inst, object, base_reg, &candidate.slot, &target)?;
        } else {
            if candidate.slot.is_declared {
                emit_uninitialized_typed_property_guard(ctx, &candidate.slot, base_reg);
            }
            emit_property_load(ctx, &candidate.slot, base_reg)?;
            box_mixed_property_candidate_result(ctx, &candidate.slot.php_type);
        }
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&stdclass_label);
    emit_stdclass_get_from_loaded_object(ctx, property);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    emit_undefined_property_warning_for_loaded_object(ctx, inst, property);
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

/// Collects declared-property candidates for a property read on an unknown `Mixed` object.
pub(super) fn declared_mixed_property_candidates(
    ctx: &FunctionContext<'_>,
    property: &str,
    inst: &Instruction,
) -> Result<Vec<MixedPropertyCandidate>> {
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
        let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
        candidates.push(MixedPropertyCandidate {
            class_id: class_info.class_id,
            slot,
        });
    }
    candidates.sort_by_key(|candidate| candidate.class_id);
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
pub(super) fn emit_mixed_property_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    candidates: &[MixedPropertyCandidate],
    match_labels: &[String],
    stdclass_label: &str,
    miss_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the receiver class id for Mixed property dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare the receiver class id against this declared-property owner
                ctx.emitter.instruction(&format!("b.eq {}", label));            // read the declared property when the class id matches
            }
            emit_branch_to_stdclass_candidate(ctx, "x9", "x10", stdclass_label);
            abi::emit_jump(ctx.emitter, miss_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r11, QWORD PTR [rax]");                // load the receiver class id for Mixed property dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", candidate.class_id as i64);
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
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    emit_property_load(ctx, &slot, base_reg)?;
    materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
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
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the property-on-null warning byte length
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &message_label);
            ctx.emitter
                .instruction(&format!("mov esi, {}", message_len)); // pass the property-on-null warning byte length
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}

/// Emits `Warning: Undefined property: Class::$name` for an object already in the result register.
pub(super) fn emit_undefined_property_warning_for_loaded_object(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
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
    emit_property_warning_fragment(ctx, b"\nWarning: Undefined property: ");
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2"),
        Arch::X86_64 => abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi"),
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    let source = ctx.module.source_path.as_deref().unwrap_or("Unknown");
    let line = inst.span.map_or(0, |span| span.line);
    emit_property_warning_fragment(
        ctx,
        format!("::${property} in {source} on line {line}\n").as_bytes(),
    );
}
