//! Purpose:
//! Lowers property reads for statically known and nullable object receivers.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Reference cells, magic getters, stdClass, and dynamic-property routes stay distinct.

use super::*;

/// Lowers a declared object property read for statically known object receivers.
pub(in crate::codegen::lower_inst) fn lower_prop_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Object(_)) {
        return lower_object_prop_get_with_null_guard(ctx, inst, object, &property);
    }
    lower_prop_get_nonnull(ctx, inst, object, &property)
}

/// Guards statically typed object receivers before selecting declared, dynamic,
/// stdClass, or magic-property lowering.
pub(super) fn lower_object_prop_get_with_null_guard(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    let null_label = ctx.next_label("prop_get_null_receiver");
    let done_label = ctx.next_label("prop_get_done");
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        base_reg,
        scratch_reg,
        &null_label,
    );
    lower_prop_get_nonnull(ctx, inst, object, property)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    if inst.op != Op::NullsafePropGet {
        emit_property_on_null_warning(ctx, property);
    }
    // Property reads keep the legacy zero-float miss shape: their null result is never
    // re-tested for null the way a silent `??` element read is.
    super::super::arrays::emit_array_get_null_fallback(ctx, &inst.result_php_type.codegen_repr(), false);
    store_if_result(ctx, inst)?;

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Selects the property representation after a statically typed object receiver
/// has been proven non-null, or for receiver shapes with their own null handling.
pub(super) fn lower_prop_get_nonnull(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    if let Some((class_name, true)) = nullable_object_receiver_class(ctx, object)? {
        return lower_nullable_prop_get_with_warning(ctx, inst, object, &class_name, property);
    }
    if let Some(class_name) = union_object_member_class(ctx, object)? {
        return lower_union_object_prop_get(ctx, inst, object, &class_name, property);
    }
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
    if object_property_is_missing(ctx, object, property)? {
        return lower_undefined_property_get(ctx, inst, object, property);
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

/// Lowers `LoadPropRefCell`: loads the raw ref-cell pointer stored in a reference
/// property's slot without dereferencing it. The result (an integer-sized pointer)
/// is the cell shared by the property; callers alias a local to it (`$x = &$obj->prop`)
/// or return it by reference (`fn &() => $this->prop`).
pub(in crate::codegen::lower_inst) fn lower_load_prop_ref_cell(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return lower_mixed_load_prop_ref_cell(ctx, inst, object, &property);
    }
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    if !slot.is_reference {
        return Err(CodegenIrError::unsupported(format!(
            "load_prop_ref_cell on non-reference property {}::${}",
            slot.class_name, slot.property
        )));
    }
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    let int_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset); // load the reference-cell pointer from the property slot (no deref)
    store_ref_cell_pointer_result(ctx, inst)
}

/// Stores the materialized reference-cell pointer (in the integer result register) into the
/// instruction's result value as a single machine word.
///
/// The cell pointer is one pointer-sized word whatever element type it aliases, so it must
/// not go through the type-driven result store (which would split a `Str`/`Float` result and
/// drop the pointer). Shared by both the typed-object and `Mixed`-receiver `LoadPropRefCell`
/// lowerings.
pub(super) fn store_ref_cell_pointer_result(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if let Some(result) = inst.result {
        ctx.store_int_result_value(result)?;
    }
    Ok(())
}

/// Lowers `LoadPropRefCell` when the receiver is a `Mixed` object (e.g. a closure's `$this`
/// bound via `Closure::bind`). Unboxes the receiver, dispatches on its runtime class id, and
/// loads the reference-cell pointer from the matching class's property slot — the raw cell
/// pointer, not its dereferenced value.
pub(super) fn lower_mixed_load_prop_ref_cell(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    let candidates: Vec<MixedPropertyCandidate> =
        declared_mixed_property_candidates(ctx, property, inst)?
            .into_iter()
            .filter(|candidate| candidate.slot.is_reference)
            .collect();
    if candidates.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "load_prop_ref_cell on Mixed receiver for property ${} with no reference-property class",
            property
        )));
    }
    let done_label = ctx.next_label("mixed_propref_done");
    let null_label = ctx.next_label("mixed_propref_null");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!("mixed_propref_{}", label_fragment(&candidate.slot.class_name)))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_mixed_object_payload_or_null(ctx, &null_label);
    // stdClass and classes without this reference property have no matching cell.
    emit_mixed_property_class_dispatch(
        ctx,
        &candidates,
        &match_labels,
        &null_label,
        &null_label,
    );

    let int_reg = abi::int_result_reg(ctx.emitter);
    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_load_from_address(ctx.emitter, int_reg, int_reg, candidate.slot.offset); // load the reference-cell pointer from the matched class's property slot
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&null_label);
    abi::emit_load_int_immediate(ctx.emitter, int_reg, 0); // no reference cell for a non-object / unknown receiver

    ctx.emitter.label(&done_label);
    store_ref_cell_pointer_result(ctx, inst)
}

/// Lowers a declared object-property initialization probe.
pub(in crate::codegen::lower_inst) fn lower_prop_initialized(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    if let Some((class_name, true)) = nullable_object_receiver_class(ctx, object)? {
        return lower_nullable_prop_initialized(ctx, inst, object, &class_name, &property);
    }
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    if !slot.is_declared {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        return store_if_result(ctx, inst);
    }
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    emit_typed_property_initialized_bool(ctx, &slot, base_reg);
    store_if_result(ctx, inst)
}

/// Probes a typed property through a NULLABLE (`?C`) receiver.
///
/// Such a receiver represents as a boxed `Mixed`, so the probe above has no object pointer to
/// read the slot from and this instruction used to be refused outright — which made
/// `isset($c->p)` a compile error and left `$c->p ?? "d"` on the ordinary read, where it fatals
/// on an uninitialized slot. The receiver is unboxed here the same way every other nullable
/// receiver is, and a NULL one answers `false`: that is the answer both callers want, since
/// `isset(null->p)` is false and `null->p ?? "d"` is the default.
///
/// The probe and the read that follows it consume the SAME unboxed value, so nothing can
/// re-null the receiver between them — the initialized branch is only ever entered with the
/// object this instruction just proved present.
fn lower_nullable_prop_initialized(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
    if !slot.is_declared {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        return store_if_result(ctx, inst);
    }
    let null_label = ctx.next_label("prop_initialized_null_receiver");
    let done_label = ctx.next_label("prop_initialized_done");
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    emit_nullable_receiver_object_payload(ctx, object, &null_label, base_reg)?;
    emit_typed_property_initialized_bool(ctx, &slot, base_reg);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0); // a null receiver has no slot to be initialized

    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Returns the receiver class when an undeclared property should route through `__get`.
pub(super) fn magic_get_receiver_class(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    property: &str,
) -> Result<Option<String>> {
    let PhpType::Object(class_name) = ctx.value_php_type(object)?.codegen_repr() else {
        return Ok(None);
    };
    let normalized = class_name.trim_start_matches('\\');
    let Some(class_info) = ctx.module.class_infos.get(normalized) else {
        return Ok(None);
    };
    if class_info
        .properties
        .iter()
        .any(|(name, _)| name == property)
    {
        return Ok(None);
    }
    if class_info.methods.contains_key(&php_symbol_key("__get")) {
        return Ok(Some(normalized.to_string()));
    }
    Ok(None)
}

/// Lowers a missing declared-property read by calling the class `__get` method.
pub(super) fn lower_magic_get_prop(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    let target = resolve_method_call_target(ctx, class_name, "__get", 2)?;
    if target.ref_params.first().copied().unwrap_or(false) {
        return Err(CodegenIrError::unsupported(format!(
            "magic __get by-reference name parameter on {}",
            class_name
        )));
    }
    emit_magic_get_args(ctx, object, property)?;
    if let Some(slot) = target.dynamic_slot {
        super::super::emit_dynamic_instance_method_call(ctx, slot);
    } else {
        abi::emit_call_label(
            ctx.emitter,
            &method_symbol(&target.impl_class, &target.method_key),
        );
    }
    store_method_call_result(ctx, inst, &target)
}

/// Loads `$this` and the static property name into ABI registers for `__get`.
pub(super) fn emit_magic_get_args(
    ctx: &mut FunctionContext<'_>,
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
    Ok(())
}

/// Lowers a named property read from a statically known stdClass receiver.
pub(super) fn lower_stdclass_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    emit_stdclass_get_call(ctx, object, property)?;
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Calls the stdClass runtime getter for an object receiver and static property name.
pub(super) fn emit_stdclass_get_call(
    ctx: &mut FunctionContext<'_>,
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
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
    Ok(())
}

/// Lowers a static-name read from an undeclared property on an allow-dynamic class.
///
/// OWNERSHIP: the miss path boxes a FRESH null cell, so the caller owns and releases the
/// result. `__rt_hash_get` only BORROWS the stored cell, so the hit path has to retain it
/// to match — exactly what `__rt_stdclass_get` does for the same storage. Without the
/// retain each read handed the caller a reference it did not own, and the caller's release
/// eventually freed a live hash entry, after which further reads of that property answered
/// `NULL` (a use-after-free of the removed cell).
pub(super) fn lower_allow_dynamic_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
    hash_offset: usize,
) -> Result<()> {
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let (label, key_len) = ctx.data.add_string(property.as_bytes());
    let miss_label = ctx.next_label("dynamic_prop_miss");
    let done_label = ctx.next_label("dynamic_prop_done");
    ctx.load_value_to_reg(object, object_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x0, [{}, #{}]", object_reg, hash_offset)); // load the dynamic-property hash pointer from the receiver
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction(&format!("cbz x0, {}", miss_label));        // missing dynamic properties read as PHP null
            ctx.emitter.instruction("mov x0, x1");                              // return the boxed Mixed cell stored in the hash entry
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the null fallback after a successful dynamic-property hit
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(
                "mov rdi, QWORD PTR [{} + {}]",
                object_reg, hash_offset
            ));                                                                 // load the dynamic-property hash pointer from the receiver
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction("test rax, rax");                           // check whether the dynamic-property key was present
            ctx.emitter.instruction(&format!("je {}", miss_label));             // missing dynamic properties read as PHP null
            ctx.emitter.instruction("mov rax, rdi");                            // return the boxed Mixed cell stored in the hash entry
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the null fallback after a successful dynamic-property hit
        }
    }
    ctx.emitter.label(&miss_label);
    emit_boxed_null(ctx);
    ctx.emitter.label(&done_label);
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}
