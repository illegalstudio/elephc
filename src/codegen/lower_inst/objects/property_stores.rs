//! Purpose:
//! Emits declared and reference-property stores with balanced ownership.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Promoted by-reference parameters retain their original ref-cell aliasing.

use super::*;

/// Emits a declared-property store from an SSA value into the object slot.
pub(super) fn emit_property_store(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    if slot.is_packed {
        return emit_packed_field_store(ctx, value, slot, base_reg);
    }
    if slot.is_reference {
        return emit_reference_property_write(ctx, value, slot, base_reg);
    }
    let value_ty = ctx.value_php_type(value)?;
    if is_pointer_sized_property_type(&slot.storage_type)
        && is_pointer_slot_null_sentinel(ctx, value, &value_ty)?
    {
        release_previous_property_value(ctx, base_reg, &slot.storage_type, slot.offset, None);
        abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset);
        abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        return Ok(());
    }
    match slot.storage_type.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            load_property_store_value_to_result(ctx, value, slot)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            release_previous_property_value(
                ctx,
                base_reg,
                &slot.storage_type,
                slot.offset,
                Some(&slot.storage_type),
            );
            abi::emit_store_to_address(ctx.emitter, ptr_reg, base_reg, slot.offset);
            abi::emit_store_to_address(ctx.emitter, len_reg, base_reg, slot.offset + 8);
        }
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            load_property_store_value_to_result(ctx, value, slot)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, float_reg, base_reg, slot.offset);
            abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        }
        PhpType::Bool | PhpType::False | PhpType::Int | PhpType::Void | PhpType::Never => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            load_property_store_value_to_result(ctx, value, slot)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, int_reg, base_reg, slot.offset);
            abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        }
        PhpType::TaggedScalar => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            load_property_store_value_to_result(ctx, value, slot)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, int_reg, base_reg, slot.offset);
            abi::emit_store_to_address(ctx.emitter, tag_reg, base_reg, slot.offset + 8);
        }
        ty if is_pointer_sized_property_type(&ty) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            load_property_store_value_to_result(ctx, value, slot)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            release_previous_property_value(
                ctx,
                base_reg,
                &slot.storage_type,
                slot.offset,
                Some(&slot.storage_type),
            );
            abi::emit_store_to_address(ctx.emitter, int_reg, base_reg, slot.offset);
            abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        }
        _ => {
            return Err(CodegenIrError::unsupported(format!(
                "property store for PHP type {:?}",
                slot.storage_type
            )))
        }
    }
    Ok(())
}

/// Republishes a sorted container into the declared property that supplied it.
///
/// Direct `krsort` lowering passes an owning transient which generic builtin cleanup releases
/// after the call. The property therefore retains the final array/hash pointer, releases its old
/// physical container through heap-kind dispatch, and stores the replacement without consulting
/// the declared packed representation. Reference properties perform the same transfer through
/// their object-owned ref-cell.
pub(super) fn store_mutated_container_property(
    ctx: &mut FunctionContext<'_>,
    object: crate::ir::ValueId,
    slot: &PropertySlot,
    value: crate::ir::ValueId,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    if !matches!(&value_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        || !matches!(slot.php_type.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. })
    {
        return Err(CodegenIrError::unsupported(format!(
            "mutated container store for {}::${} from PHP type {:?} to {:?}",
            slot.class_name, slot.property, value_ty, slot.php_type
        )));
    }

    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    let value_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    abi::emit_push_reg(ctx.emitter, base_reg);
    ctx.load_value_to_reg(value, value_reg)?;
    abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
    abi::emit_pop_reg(ctx.emitter, base_reg);
    if slot.is_reference {
        let pointer_reg = reference_pointer_reg(ctx, base_reg);
        abi::emit_load_from_address(ctx.emitter, pointer_reg, base_reg, slot.offset);
        release_previous_referenced_value(ctx, pointer_reg, &slot.php_type, Some(&value_ty));
        abi::emit_store_to_address(ctx.emitter, value_reg, pointer_reg, 0);
    } else {
        release_previous_property_value(ctx, base_reg, &slot.php_type, slot.offset, Some(&value_ty));
        abi::emit_store_to_address(ctx.emitter, value_reg, base_reg, slot.offset);
    }
    Ok(())
}

/// Emits a promoted constructor-property bind by storing the parameter ref-cell address.
pub(super) fn emit_reference_property_bind(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    super::super::materialize_local_ref_arg_address(ctx, value)?;
    abi::emit_store_to_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        base_reg,
        slot.offset,
    );
    abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
    Ok(())
}

/// Returns true for the constructor-promotion bind pattern `$this->x = $x`.
pub(super) fn is_promoted_reference_property_bind(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    slot: &PropertySlot,
) -> Result<bool> {
    if !slot.is_reference {
        return Ok(false);
    }
    let Some(object_source) = loaded_local_source(ctx, object)? else {
        return Ok(false);
    };
    if !local_slot_name_is(ctx, object_source.slot, "this") {
        return Ok(false);
    }
    let Some(value_source) = loaded_local_source(ctx, value)? else {
        return Ok(false);
    };
    if !local_slot_name_is(ctx, value_source.slot, &slot.property) {
        return Ok(false);
    }
    Ok(value_source.is_ref_cell && local_slot_is_by_ref_param(ctx, value_source.slot))
}

/// Describes an SSA value that was produced by loading an addressable local slot.
struct LoadedLocalSource {
    slot: LocalSlotId,
    is_ref_cell: bool,
}

/// Resolves a loaded SSA value back to its source local slot when possible.
fn loaded_local_source(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<LoadedLocalSource>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    let is_ref_cell = match inst_ref.op {
        Op::LoadLocal => false,
        Op::LoadRefCell => true,
        _ => return Ok(None),
    };
    let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "loaded local value has no local slot immediate",
        ));
    };
    Ok(Some(LoadedLocalSource { slot, is_ref_cell }))
}

/// Returns true when a local slot has the requested PHP source name.
pub(super) fn local_slot_name_is(ctx: &FunctionContext<'_>, slot: LocalSlotId, expected: &str) -> bool {
    ctx.function
        .locals
        .get(slot.as_raw() as usize)
        .and_then(|local| local.name.as_deref())
        .is_some_and(|name| name == expected)
}

/// Returns true when a local slot is the storage slot for a by-reference parameter.
pub(super) fn local_slot_is_by_ref_param(ctx: &FunctionContext<'_>, slot: LocalSlotId) -> bool {
    ctx.function
        .params
        .get(slot.as_raw() as usize)
        .is_some_and(|param| param.by_ref)
}

/// Emits an assignment through a reference property's stored ref-cell pointer.
pub(super) fn emit_reference_property_write(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    abi::emit_push_reg(ctx.emitter, base_reg);
    load_property_store_value_to_result(ctx, value, slot)?;
    abi::emit_pop_reg(ctx.emitter, base_reg);
    let pointer_reg = reference_pointer_reg(ctx, base_reg);
    abi::emit_load_from_address(ctx.emitter, pointer_reg, base_reg, slot.offset);
    release_previous_referenced_value(
        ctx,
        pointer_reg,
        &slot.storage_type,
        Some(&slot.storage_type),
    );
    store_current_result_to_reference_cell(ctx, pointer_reg, &slot.storage_type)
}

/// Releases the old value held in a reference cell before overwriting it.
pub(super) fn release_previous_referenced_value(
    ctx: &mut FunctionContext<'_>,
    pointer_reg: &str,
    prop_ty: &PhpType,
    preserve_result_ty: Option<&PhpType>,
) {
    let prop_ty = prop_ty.codegen_repr();
    let releases_value =
        matches!(prop_ty, PhpType::Str | PhpType::Callable) || prop_ty.is_refcounted();
    if !releases_value {
        return;
    }
    if let Some(result_ty) = preserve_result_ty {
        abi::emit_push_result_value(ctx.emitter, &result_ty.codegen_repr());
    }
    abi::emit_push_reg(ctx.emitter, pointer_reg);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        pointer_reg,
        0,
    );
    match prop_ty {
        PhpType::Str => abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe"),
        PhpType::Callable => callable_descriptor::emit_release_current_descriptor(ctx.emitter),
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
        }
        ty => abi::emit_decref_if_refcounted(ctx.emitter, &ty),
    }
    abi::emit_pop_reg(ctx.emitter, pointer_reg);
    if let Some(result_ty) = preserve_result_ty {
        restore_property_store_result(ctx, &result_ty.codegen_repr());
    }
}

/// Stores the current result registers into a reference cell.
pub(super) fn store_current_result_to_reference_cell(
    ctx: &mut FunctionContext<'_>,
    pointer_reg: &str,
    prop_ty: &PhpType,
) -> Result<()> {
    match prop_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, pointer_reg, 0);
            abi::emit_store_to_address(ctx.emitter, len_reg, pointer_reg, 8);
        }
        PhpType::Float => {
            abi::emit_store_to_address(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                pointer_reg,
                0,
            );
        }
        PhpType::TaggedScalar => {
            abi::emit_store_to_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                pointer_reg,
                0,
            );
            abi::emit_store_to_address(
                ctx.emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
                pointer_reg,
                8,
            );
        }
        ty if is_pointer_sized_property_type(&ty)
            || matches!(
                ty,
                PhpType::Bool | PhpType::Int | PhpType::Void | PhpType::Never
            ) =>
        {
            abi::emit_store_to_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                pointer_reg,
                0,
            );
        }
        ty => {
            return Err(CodegenIrError::unsupported(format!(
                "reference property store for PHP type {:?}",
                ty
            )))
        }
    }
    Ok(())
}

/// Returns a scratch register that can hold a reference-cell pointer beside the object base.
pub(super) fn reference_pointer_reg(ctx: &FunctionContext<'_>, base_reg: &str) -> &'static str {
    let symbol_reg = abi::symbol_scratch_reg(ctx.emitter);
    if base_reg == symbol_reg {
        abi::secondary_scratch_reg(ctx.emitter)
    } else {
        symbol_reg
    }
}

/// Releases the old value in a declared property slot before overwriting it.
pub(super) fn release_previous_property_value(
    ctx: &mut FunctionContext<'_>,
    base_reg: &str,
    prop_ty: &PhpType,
    offset: usize,
    preserve_result_ty: Option<&PhpType>,
) {
    let prop_ty = prop_ty.codegen_repr();
    let releases_value =
        matches!(prop_ty, PhpType::Str | PhpType::Callable) || prop_ty.is_refcounted();
    if !releases_value {
        return;
    }
    if let Some(result_ty) = preserve_result_ty {
        abi::emit_push_result_value(ctx.emitter, &result_ty.codegen_repr());
    }
    abi::emit_push_reg(ctx.emitter, base_reg);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        base_reg,
        offset,
    );
    match prop_ty {
        PhpType::Str => abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe"),
        PhpType::Callable => callable_descriptor::emit_release_current_descriptor(ctx.emitter),
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
        }
        ty => abi::emit_decref_if_refcounted(ctx.emitter, &ty),
    }
    abi::emit_pop_reg(ctx.emitter, base_reg);
    if let Some(result_ty) = preserve_result_ty {
        restore_property_store_result(ctx, &result_ty.codegen_repr());
    }
}

/// Restores a property-store result value saved around previous-slot release.
pub(super) fn restore_property_store_result(ctx: &mut FunctionContext<'_>, result_ty: &PhpType) {
    match result_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        }
    }
}
