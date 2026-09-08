//! Purpose:
//! Lowers fetch-for-write reads of fixed object-property container slots.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` through the object facade.
//!
//! Key details:
//! - Separates array/hash storage before by-reference iteration and writes the
//!   result back through either a direct property slot or its reference cell.
//! - Mixed roots detach their zval before nested writes, preserving other value copies.
//! - Unsupported slot shapes are hard errors because the EIR result is borrowed.

use super::*;

/// Lowers `PropGetForWrite`: separates the property's container before a by-reference
/// `foreach` iterates it, publishes the separated container back into the property slot, and
/// leaves it in the result register borrowed (issue #642).
///
/// This is the property-side counterpart of `ArrayGetForWrite`. `__rt_array_ensure_unique`
/// consumes one reference from a shared source when it splits and hands back a container at
/// refcount 1; storing that container into the property slot is therefore exactly balanced —
/// the reference the property already held is the one the split consumed. No extra retain is
/// emitted, which is the whole point: the loop must iterate storage the property owns, without
/// a second reference that would make `IterStart` copy it and the loop exit over-release it.
///
/// There is deliberately no fallback. A slot this function cannot split is an `unsupported`
/// codegen error, not a plain read: the frontend has already marked the result `Borrowed`, so
/// skipping the split leaves the loop borrowing a shared container whose split inside `IterStart`
/// consumes the reference the property holds. Failing loudly keeps the frontend and backend
/// slot classifiers coherent.
pub(in crate::codegen::lower_inst) fn lower_prop_get_for_write(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    let Some(split) = property_container_split(&slot) else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for property {}::${} with PHP type {:?}",
            inst.op.name(),
            slot.class_name,
            slot.property,
            slot.php_type
        )));
    };
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let arg_reg = if split.boxed {
        result_reg
    } else {
        abi::int_arg_reg_name(ctx.emitter.target, 0)
    };
    ctx.load_value_to_reg(object, base_reg)?;
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    abi::emit_load_from_address(ctx.emitter, arg_reg, base_reg, slot.offset);
    if split.through_reference_cell {
        // A reference slot stores the ref-cell pointer, not the container. The container the
        // loop must iterate lives at offset 0 inside the cell, and that is also where the
        // separated one is published so every alias of the reference observes it.
        abi::emit_load_from_address(ctx.emitter, arg_reg, arg_reg, 0);
    }
    if split.boxed {
        abi::emit_push_reg(ctx.emitter, result_reg);
        abi::emit_call_label(ctx.emitter, split.helper);
        abi::emit_push_reg(ctx.emitter, result_reg);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        abi::emit_pop_reg(ctx.emitter, result_reg);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    } else {
        abi::emit_call_label(ctx.emitter, split.helper);
    }
    // The split helper clobbers the scratch registers on both targets, so reload the receiver
    // before publishing the separated container into its slot.
    ctx.load_value_to_reg(object, base_reg)?;
    if split.through_reference_cell {
        let cell_reg = reference_pointer_reg(ctx, base_reg);
        abi::emit_load_from_address(ctx.emitter, cell_reg, base_reg, slot.offset);
        abi::emit_store_to_address(ctx.emitter, result_reg, cell_reg, 0);
    } else {
        abi::emit_store_to_address(ctx.emitter, result_reg, base_reg, slot.offset);
    }
    store_if_result(ctx, inst)
}

/// Describes how `PropGetForWrite` reaches and republishes one property's container.
struct PropertyContainerSplit {
    /// The copy-on-write helper that separates this container kind.
    helper: &'static str,
    /// Whether the property slot holds a reference cell containing the container pointer.
    through_reference_cell: bool,
    /// Mixed cloning acquires the payload but leaves the old zval owner for explicit release.
    boxed: bool,
}

/// Classifies a property slot as a container `PropGetForWrite` can split.
///
/// Indexed and associative containers are supported whether the property is typed or untyped.
/// Reference slots add one indirection through their cell. Packed fields and scalar properties
/// cannot be split by this operation.
fn property_container_split(slot: &PropertySlot) -> Option<PropertyContainerSplit> {
    if slot.is_packed {
        return None;
    }
    let helper = match slot.php_type.codegen_repr() {
        PhpType::Array(_) => "__rt_array_ensure_unique",
        PhpType::AssocArray { .. } => "__rt_hash_ensure_unique",
        PhpType::Mixed => "__rt_mixed_clone",
        _ => return None,
    };
    Some(PropertyContainerSplit {
        helper,
        through_reference_cell: slot.is_reference,
        boxed: slot.php_type.codegen_repr() == PhpType::Mixed,
    })
}
