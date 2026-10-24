//! Purpose:
//! Lowers scalar comparisons and local or ref-cell loads.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.
//! - EIR construction and codegen share the local-storage compatibility predicate.

use super::*;

/// Lowers a signed integer comparison into a boolean result value.
pub(super) fn lower_int_compare(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let predicate = immediate_operands::emit_int_compare(ctx, inst)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(                                   // materialize the predicate result as 0 or 1
                "cset {}, {}",
                result_reg,
                aarch64_condition(predicate)?
            ));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(                                   // materialize the predicate result in the low byte
                "set{} al", x86_64_condition(predicate)?
            ));
            ctx.emitter.instruction(&format!(                                   // widen the predicate byte into the integer result register
                "movzx {}, al", result_reg
            ));
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers an addressable local load into the result register and SSA destination slot.
pub(super) fn lower_load_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("load_local missing result value"))?;
    ctx.load_local_to_result(slot)?;
    // The slot's box type, so a `?ptr<T>` slot boxed for a `mixed` result reads null for 0.
    let source_ty = ctx.local_mixed_box_type(slot)?;
    let result_ty = ctx.value_php_type(result)?;
    coerce_loaded_local_to_result_type(ctx, &source_ty, &result_ty)?;
    ctx.store_result_value(result)
}

/// Lowers an explicit local ref-cell load into the result register and SSA slot.
pub(super) fn lower_load_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("load_ref_cell missing result value"))?;
    let result_ty = ctx.value_php_type(result)?;
    if ctx.local_ref_cell_representation_is_definite(slot) {
        load_ref_cell_local_to_result_as(ctx, slot, &result_ty)?;
        return ctx.store_result_value(result);
    }
    if !ctx.local_ref_cell_representation_is_dynamic(slot) {
        let source_ty = ctx.load_raw_local_to_result(slot)?;
        coerce_borrowed_ref_cell_local(ctx, &source_ty, &result_ty)?;
        return ctx.store_result_value(result);
    }
    let state_offset = ctx.ref_cell_state_offset(slot).ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "dynamic ref-cell slot {} has no representation flag",
            slot.as_raw()
        ))
    })?;
    let ref_cell = ctx.next_label("dynamic_load_ref_cell");
    let done = ctx.next_label("dynamic_load_ref_cell_done");
    let state_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, state_reg, state_offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(                                            // select the alias representation after runtime promotion
                &format!("cbnz {}, {}", state_reg, ref_cell)
            );
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(                                            // test the slot's runtime representation flag
                &format!("test {}, {}", state_reg, state_reg)
            );
            ctx.emitter.instruction(&format!("jne {}", ref_cell));              // select the alias representation after runtime promotion
        }
    }
    let source_ty = ctx.load_raw_local_to_result(slot)?;
    coerce_borrowed_ref_cell_local(ctx, &source_ty, &result_ty)?;
    ctx.emit_branch(&done);
    ctx.emitter.label(&ref_cell);
    load_ref_cell_local_to_result_as(ctx, slot, &result_ty)?;
    ctx.emitter.label(&done);
    ctx.store_result_value(result)
}

/// Keeps a ref-cell read borrowed on the raw side of a conditional promotion.
/// The cell path already borrows its payload; a flow-proven heap view from an unpromoted
/// Mixed box must borrow the payload directly instead of adding a lease or string copy.
pub(super) fn coerce_borrowed_ref_cell_local(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    if source_ty.codegen_repr() == PhpType::Mixed
        && matches!(result_ty.codegen_repr(), PhpType::Str | PhpType::Array(_)
            | PhpType::AssocArray { .. } | PhpType::Object(_) | PhpType::Callable
            | PhpType::Iterable)
    {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        move_reg_to_int_result(ctx, mixed_unbox_low_payload_reg(ctx));
        return Ok(());
    }
    coerce_loaded_local_to_result_type(ctx, source_ty, result_ty)
}

/// Loads the value pointed to by a local ref-cell slot using the supplied alias type.
pub(super) fn load_ref_cell_local_to_result_as(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    ty: &PhpType,
) -> Result<PhpType> {
    let ty = ty.codegen_repr();
    reject_multiword_ref_param_local(&ty, "load")?;
    let offset = ctx.local_offset(slot)?;
    let pointer_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, pointer_reg, offset);
    match ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, ptr_reg, pointer_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, pointer_reg, 8);
        }
        PhpType::Float => {
            abi::emit_load_from_address(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                pointer_reg,
                0,
            );
        }
        PhpType::TaggedScalar => {
            abi::emit_load_from_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                pointer_reg,
                0,
            );
            abi::emit_load_from_address(
                ctx.emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
                pointer_reg,
                8,
            );
        }
        _ => {
            abi::emit_load_from_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                pointer_reg,
                0,
            );
        }
    }
    Ok(ty)
}

/// Converts a loaded local slot value to the SSA result representation requested by EIR.
pub(in crate::codegen) fn coerce_loaded_local_to_result_type(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    let box_ty = crate::types::mixed_box_php_type(source_ty);
    let source_ty = source_ty.codegen_repr();
    let result_ty = result_ty.codegen_repr();
    if crate::ir::local_load_types_share_storage(&source_ty, &result_ty) {
        return Ok(());
    }
    match (&source_ty, &result_ty) {
        // A boxed pointer is an integer-like payload and a boxed null casts to 0, which is the
        // null of a `?ptr<T>` read (a `mixed` static local later holding a pointer).
        (PhpType::Mixed, PhpType::Int | PhpType::Pointer(_)) => {
            move_int_result_to_first_arg(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        (PhpType::Mixed, PhpType::Bool) => {
            move_int_result_to_first_arg(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            Ok(())
        }
        (PhpType::Mixed, PhpType::Float) => {
            move_int_result_to_first_arg(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            Ok(())
        }
        (PhpType::Mixed, PhpType::Str) => {
            move_int_result_to_first_arg(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            Ok(())
        }
        (PhpType::Mixed, PhpType::Array(_))
        | (PhpType::Mixed, PhpType::AssocArray { .. })
        | (PhpType::Mixed, PhpType::Callable)
        | (PhpType::Mixed, PhpType::Object(_)) => {
            emit_unbox_mixed_to_owned_refcounted_result(ctx, &result_ty);
            Ok(())
        }
        (PhpType::Mixed, PhpType::Iterable) => {
            emit_unbox_mixed_to_owned_refcounted_result(ctx, &result_ty);
            Ok(())
        }
        (PhpType::Mixed, PhpType::Void) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            Ok(())
        }
        // A pointer-word local that lowering knows holds `null` here (address 0 in the slot)
        // reads as the ordinary null value every `null` consumer expects.
        (PhpType::Pointer(_), PhpType::Void) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                crate::codegen::sentinels::NULL_SENTINEL,
            );
            Ok(())
        }
        (_, PhpType::TaggedScalar) => {
            coerce_loaded_value_to_tagged_scalar(ctx, &source_ty)?;
            Ok(())
        }
        // A `?ptr<T>` source keeps its union, so address 0 is PHP null.
        (_, PhpType::Mixed) => {
            emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
            Ok(())
        }
        _ => Err(CodegenIrError::unsupported(format!(
            "local load from PHP type {:?} as {:?}",
            source_ty, result_ty
        ))),
    }
}
