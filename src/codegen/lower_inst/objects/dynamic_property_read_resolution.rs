//! Purpose:
//! Resolves runtime property names against stdClass and declared slots.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Candidate type compatibility and miss materialization remain explicit.

use super::*;

/// Lowers a runtime-name dynamic property read from a statically known `stdClass`.
pub(super) fn lower_runtime_dynamic_stdclass_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property_value: ValueId,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            ctx.load_string_value_to_regs(property_value, "x1", "x2")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            ctx.load_string_value_to_regs(property_value, "rsi", "rdx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Lowers a runtime string dynamic property read by dispatching across declared slots.
pub(super) fn lower_runtime_dynamic_declared_prop_get(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    let class_name = dynamic_property_object_class(ctx, object, inst)?;
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    ensure_dynamic_property_miss_supported(inst)?;
    let slots = declared_dynamic_property_slots(ctx, &class_name, inst)?;
    ensure_dynamic_property_slot_results_supported(&slots, inst)?;
    let match_labels = slots
        .iter()
        .map(|slot| ctx.next_label(&format!("dyn_prop_{}", label_fragment(&slot.property))))
        .collect::<Vec<_>>();
    let miss_label = ctx.next_label("dyn_prop_miss");
    let done_label = ctx.next_label("dyn_prop_done");

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
        if let Some(opcode) = runtime_dom_property_opcode_for_slot(ctx, slot) {
            let receiver_reg = abi::int_result_reg(ctx.emitter).to_string();
            abi::emit_load_temporary_stack_slot(ctx.emitter, &receiver_reg, 16);
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            let property_inst = Instruction {
                operands: vec![inst.operands[0]],
                ..inst.clone()
            };
            super::super::internal_extensions::lower_mixed_receiver_internal_extension_call(
                ctx,
                &property_inst,
                &receiver_reg,
                opcode,
                &slot.php_type,
            )?;
        } else {
            if slot.is_declared {
                emit_uninitialized_typed_property_guard(ctx, slot, base_reg);
            }
            emit_property_load(ctx, slot, base_reg)?;
            materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
            abi::emit_release_temporary_stack(ctx.emitter, 32);
        }
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    emit_dynamic_property_miss_result(ctx, inst);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Returns the normalized class name for object receivers supported by dynamic property dispatch.
pub(super) fn dynamic_property_object_class(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    inst: &Instruction,
) -> Result<String> {
    let object_ty = ctx.value_php_type(object)?;
    let PhpType::Object(class_name) = object_ty else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for runtime dynamic receiver PHP type {:?}",
            inst.op.name(),
            object_ty
        )));
    };
    Ok(class_name.trim_start_matches('\\').to_string())
}

/// Verifies that the dynamic property name is already materialized as a string.
pub(super) fn ensure_runtime_dynamic_property_name(
    ctx: &FunctionContext<'_>,
    property_value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    let property_ty = ctx.value_php_type(property_value)?;
    if property_ty == PhpType::Str {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} with runtime property name PHP type {:?}",
        inst.op.name(),
        property_ty
    )))
}

/// Resolves all declared property slots that a runtime dynamic property read may match.
pub(super) fn declared_dynamic_property_slots(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<Vec<PropertySlot>> {
    let normalized = class_name.trim_start_matches('\\');
    let property_names = {
        let class_info =
            ctx.module.class_infos.get(normalized).ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", normalized))
            })?;
        class_info
            .properties
            .iter()
            .map(|(property, _)| property.clone())
            .collect::<Vec<_>>()
    };
    property_names
        .iter()
        .map(|property| resolve_property_slot_for_class(ctx, normalized, property, inst))
        .collect()
}

/// Collects declared-property candidates readable from a boxed Mixed receiver.
pub(super) fn declared_mixed_property_get_candidates(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    inst: &Instruction,
) -> Result<Vec<MixedPropertyCandidate>> {
    let receiver_bases = dynamic_property_receiver_object_bases(ctx, object)?;
    let mut candidates = Vec::new();
    let mut sorted_classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in sorted_classes {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
            continue;
        }
        if receiver_bases.as_ref().is_some_and(|bases| {
            !bases
                .iter()
                .any(|base| dynamic_property_class_is_a(ctx, class_name, base))
        }) {
            continue;
        }
        for (property, _) in &class_info.properties {
            let Ok(slot) = resolve_property_slot_for_class(ctx, class_name, property, inst) else {
                continue;
            };
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

/// Resolves shared virtual DOM node properties for a constrained boxed receiver.
pub(super) fn dynamic_internal_extension_properties(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    inst: &Instruction,
) -> Result<Vec<(PropertySlot, u32)>> {
    let Some(bases) = dynamic_property_receiver_object_bases(ctx, object)? else {
        return Ok(Vec::new());
    };
    if bases.is_empty()
        || bases
            .iter()
            .any(|base| !crate::internal_extensions::is_native_wrapper_class(base))
    {
        return Ok(Vec::new());
    }
    let properties = [
        "firstChild",
        "lastChild",
        "parentNode",
        "parentElement",
        "ownerDocument",
        "previousSibling",
        "nextSibling",
        "textContent",
        "childNodes",
    ];
    let mut resolved = Vec::new();
    for property in properties {
        let mut shared: Option<(PropertySlot, u32)> = None;
        for base in &bases {
            let Ok(slot) = resolve_property_slot_for_class(ctx, base, property, inst) else {
                shared = None;
                break;
            };
            let Some(opcode) = runtime_dom_property_opcode_for_slot(ctx, &slot) else {
                shared = None;
                break;
            };
            if shared.as_ref().is_some_and(|(current, current_opcode)| {
                *current_opcode != opcode || current.php_type != slot.php_type
            }) {
                shared = None;
                break;
            }
            shared.get_or_insert((slot, opcode));
        }
        if let Some(shared) = shared {
            resolved.push(shared);
        }
    }
    Ok(resolved)
}

/// Returns object members that constrain one boxed union receiver's runtime classes.
fn dynamic_property_receiver_object_bases(
    ctx: &FunctionContext<'_>,
    object: ValueId,
) -> Result<Option<Vec<String>>> {
    let raw_type = ctx.raw_value_php_type(object)?;
    match raw_type {
        PhpType::Object(class_name) => {
            Ok(Some(vec![class_name.trim_start_matches('\\').to_string()]))
        }
        PhpType::Union(members) => {
            let mut bases = Vec::new();
            for member in members {
                if let PhpType::Object(class_name) = member {
                    bases.push(class_name.trim_start_matches('\\').to_string());
                }
            }
            Ok((!bases.is_empty()).then_some(bases))
        }
        _ => Ok(None),
    }
}

/// Reports whether one runtime candidate class equals or extends a constrained base.
fn dynamic_property_class_is_a(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    base: &str,
) -> bool {
    let base = base.trim_start_matches('\\');
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(class_name) = current {
        if class_name.eq_ignore_ascii_case(base) {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(class_name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Verifies that the EIR result type can receive every declared property candidate.
pub(super) fn ensure_dynamic_property_slot_results_supported(
    slots: &[PropertySlot],
    inst: &Instruction,
) -> Result<()> {
    let result_ty = inst.result_php_type.codegen_repr();
    if result_ty == PhpType::Mixed {
        return Ok(());
    }
    for slot in slots {
        let slot_ty = slot.php_type.codegen_repr();
        let can_tag_nullable_int = result_ty == PhpType::TaggedScalar && slot_ty == PhpType::Int;
        if slot_ty != result_ty && !can_tag_nullable_int {
            return Err(CodegenIrError::unsupported(format!(
                "{} with declared property {}::${} PHP type {:?} and result PHP type {:?}",
                inst.op.name(),
                slot.class_name,
                slot.property,
                slot.php_type,
                result_ty
            )));
        }
    }
    Ok(())
}

/// Verifies that a runtime miss can be materialized in the EIR result register shape.
pub(super) fn ensure_dynamic_property_miss_supported(inst: &Instruction) -> Result<()> {
    match inst.result_php_type.codegen_repr() {
        PhpType::Mixed | PhpType::TaggedScalar | PhpType::Bool | PhpType::Int => Ok(()),
        ty => Err(CodegenIrError::unsupported(format!(
            "{} runtime miss for result PHP type {:?}",
            inst.op.name(),
            ty
        ))),
    }
}

/// Converts a just-loaded property payload into the EIR result representation.
pub(super) fn materialize_loaded_property_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source_ty: &PhpType,
) -> Result<()> {
    let source_ty = source_ty.codegen_repr();
    match inst.result_php_type.codegen_repr() {
        PhpType::Mixed if source_ty == PhpType::Mixed => {
            abi::emit_incref_if_refcounted(ctx.emitter, &source_ty);
            Ok(())
        }
        PhpType::Mixed => {
            emit_box_current_value_as_mixed(ctx.emitter, &source_ty);
            Ok(())
        }
        PhpType::TaggedScalar if source_ty != PhpType::TaggedScalar => {
            super::super::coerce_loaded_value_to_tagged_scalar(ctx, &source_ty)?;
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Emits a PHP null value for a dynamic property lookup that matched no declared slot.
pub(super) fn emit_dynamic_property_miss_result(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    match inst.result_php_type.codegen_repr() {
        PhpType::Mixed => emit_boxed_null(ctx),
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
        }
        _ => abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            RUNTIME_NULL_SENTINEL,
        ),
    }
}

/// Emits a runtime string comparison branch against one declared property name.
pub(super) fn emit_branch_if_dynamic_name_matches(
    ctx: &mut FunctionContext<'_>,
    property: &str,
    target_label: &str,
) {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            abi::emit_symbol_address(ctx.emitter, "x3", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", len as i64);
            ctx.emitter.instruction("bl __rt_str_eq");                          // compare the runtime property name against this declared property
            ctx.emitter
                .instruction(&format!("cbnz x0, {}", target_label)); // dispatch to the declared property slot when the names match
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
            abi::emit_symbol_address(ctx.emitter, "rdx", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", len as i64);
            ctx.emitter.instruction("call __rt_str_eq");                        // compare the runtime property name against this declared property
            ctx.emitter.instruction("test rax, rax");                           // check whether the runtime string comparison matched
            ctx.emitter.instruction(&format!("jne {}", target_label));          // dispatch to the declared property slot when the names match
        }
    }
}

/// Converts arbitrary names into assembly-label-safe fragments.
pub(super) fn label_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}
