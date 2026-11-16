//! Purpose:
//! Publishes original local and static operands through canonical native references.
//!
//! Called from:
//! - Local and static assignment lowering when native reference validation is selected.
//!
//! Key details:
//! - Raw and unpromoted cells retain their existing store path.
//! - The publisher consumes one boxed RHS owner and the assignment-site strictness.
//! - Extra source parking ends before publication, while the replacement still owns its payload.

use super::*;

const SCRATCH_BYTES: usize = 128;
const WRAPPER: usize = 0;
const CONTEXT: usize = 16;
const PENDING: usize = CONTEXT + 8;
const OUTCOME: usize = 32;
const RHS: usize = 112;

/// Emits a canonical path and returns the join label to place after the unchanged raw fallback.
pub(super) fn emit_if_canonical(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, slot: LocalSlotId, value: ValueId,
) -> Result<Option<String>> {
    if !ctx.module.required_runtime_features.native_reference_validation
        || inst.operands.len() < 2
        || (!ctx.local_ref_cell_representation_is_definite(slot)
            && !ctx.local_ref_cell_representation_is_dynamic(slot))
    { return Ok(None); }
    let raw = ctx.next_label("native_reference_store_raw");
    let done = ctx.next_label("native_reference_store_done");
    let result = abi::int_result_reg(ctx.emitter);
    if ctx.local_ref_cell_representation_is_dynamic(slot) {
        let state = ctx.ref_cell_state_offset(slot).ok_or_else(||
            CodegenIrError::invalid_module("dynamic native reference store has no representation flag"))?;
        abi::load_at_offset(ctx.emitter, result, state);
        abi::emit_branch_if_int_result_zero(ctx.emitter, &raw);
    }
    let offset = ctx.local_offset(slot)?;
    abi::load_at_offset(ctx.emitter, result, offset);
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_canonical");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &raw);
    abi::emit_reserve_temporary_stack(ctx.emitter, SCRATCH_BYTES);
    abi::emit_store_to_sp(ctx.emitter, result, WRAPPER);
    ctx.load_value_to_result(inst.operands[1])?;
    abi::emit_store_to_sp(ctx.emitter, result, CONTEXT);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    abi::emit_store_to_sp(ctx.emitter, result, PENDING);

    let source = ctx.mixed_box_type(value)?;
    let owned = ctx.value_can_own_mixed_box_source(value)?;
    ctx.load_value_to_result(value)?;
    if owned {
        emit_box_current_owned_value_as_mixed(ctx.emitter, &source);
    } else if source.codegen_repr() == PhpType::Mixed {
        // Ordinary boxing leaves a Mixed pointer borrowed. This entry consumes its own owner.
        abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &source);
    }
    abi::emit_store_to_sp(ctx.emitter, result, RHS);
    retire_source_parking(ctx, inst)?;

    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), CONTEXT);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), WRAPPER);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), RHS);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), OUTCOME);
    abi::emit_call_label(ctx.emitter, "__rt_reference_publish_native");
    emit_status(ctx, &done);
    ctx.emitter.label(&raw);
    Ok(Some(done))
}

/// Consumes the current owned Mixed result when a static binding already owns a canonical cell.
/// The static parking record borrows this same owner, so detaching it transfers rather than releases.
pub(in crate::codegen::lower_inst) fn emit_static_if_managed(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, symbol: &str, transfer_owner: Option<usize>,
) -> Result<Option<String>> {
    if !ctx.module.required_runtime_features.native_reference_validation || inst.operands.len() < 2 {
        return Ok(None);
    }
    let raw = ctx.next_label("native_static_reference_store_raw");
    let done = ctx.next_label("native_static_reference_store_done");
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_reserve_temporary_stack(ctx.emitter, SCRATCH_BYTES);
    abi::emit_store_to_sp(ctx.emitter, result, RHS);
    abi::emit_load_symbol_to_reg(ctx.emitter, result, symbol, 16);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &raw);
    abi::emit_load_symbol_to_reg(ctx.emitter, result, symbol, 0);
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_canonical");
    abi::emit_store_to_sp(ctx.emitter, result, WRAPPER);
    ctx.load_value_to_result(inst.operands[1])?;
    abi::emit_store_to_sp(ctx.emitter, result, CONTEXT);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    abi::emit_store_to_sp(ctx.emitter, result, PENDING);
    if let Some(offset) = transfer_owner {
        abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
    }
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), CONTEXT);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), WRAPPER);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), RHS);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), OUTCOME);
    abi::emit_call_label(ctx.emitter, "__rt_reference_publish_native");
    emit_status(ctx, &done);
    ctx.emitter.label(&raw);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, RHS);
    abi::emit_release_temporary_stack(ctx.emitter, SCRATCH_BYTES);
    Ok(Some(done))
}

/// Consumes the old source lease while the new owned box makes payload retirement non-final.
fn retire_source_parking(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let Some(owner) = inst.operands.get(2).copied() else { return Ok(()); };
    let Some(load) = instruction_for_value(ctx, owner)? else {
        return Err(CodegenIrError::invalid_module("reference source owner must load a parking slot"));
    };
    let Some(Immediate::LocalSlot(slot)) = load.immediate else {
        return Err(CodegenIrError::invalid_module("reference source owner has no parking slot"));
    };
    if load.op != Op::LoadLocal || !matches!(ctx.local_kind(slot)?, LocalKind::OwnedTemp | LocalKind::HiddenTemp)
        || ctx.value_ownership(owner)? != Ownership::Borrowed
    { return Err(CodegenIrError::invalid_module("reference source owner is not a borrowed unwind slot")); }
    let offset = ctx.local_offset(slot)?;
    let ty = ctx.local_php_type(slot)?.codegen_repr();
    abi::load_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
    if matches!(ty, PhpType::Str | PhpType::TaggedScalar) {
        abi::emit_store_zero_to_local_slot(ctx.emitter, offset - 8);
    }
    if ty == PhpType::Str {
        abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    } else {
        abi::emit_decref_if_refcounted(ctx.emitter, &ty);
    }
    Ok(())
}

/// Restores the temporary stack before propagating a contained Throwable or a source conflict.
fn emit_status(ctx: &mut FunctionContext<'_>, done: &str) {
    let result = abi::int_result_reg(ctx.emitter);
    let success = ctx.next_label("native_reference_store_success");
    let invalid = ctx.next_label("native_reference_store_invalid");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &success);
    abi::emit_store_to_sp(ctx.emitter, result, WRAPPER);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &invalid);
    let older = match ctx.emitter.target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rdi" };
    abi::emit_load_symbol_to_reg(ctx.emitter, older, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_exc_value", 0);
    abi::emit_call_label(ctx.emitter, "__rt_exception_chain");
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(ctx.emitter, result, "_exc_value", 0);
    abi::emit_release_temporary_stack(ctx.emitter, SCRATCH_BYTES);
    abi::emit_jump(ctx.emitter, "__rt_throw_current");

    ctx.emitter.label(&invalid);
    abi::emit_release_temporary_stack(ctx.emitter, SCRATCH_BYTES);
    super::super::exceptions::emit_type_error(ctx,
        "Cannot assign to a reference held by properties with incompatible types");
    ctx.emitter.label(&success);
    abi::emit_release_temporary_stack(ctx.emitter, SCRATCH_BYTES);
    abi::emit_jump(ctx.emitter, done);
}
