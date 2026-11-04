//! Purpose:
//! Lowers boxed loose comparisons while preserving live numeric reference cells.
//!
//! Called from:
//! - The loose equality dispatcher in `super`.
//!
//! Key details:
//! - Concrete adaptation boxes have balanced normal and exception cleanup.
//! - A numeric conversion warning may replace the box held by a reference cell.
//!   The runtime receives its storage address and reloads it after the callback.

use super::{decref_mixed_temp_at, is_mixed_like, materialize_value_as_mixed};
use crate::codegen::{abi, platform::Arch, CodegenIrError, Result};
use crate::codegen::context::FunctionContext;
use crate::ir::{Immediate, LocalSlotId, Op, ValueId};
use crate::types::PhpType;

/// Emits PHP loose equality through `__rt_mixed_loose_eq` for operand pairs whose
/// rule cannot be decided from the static types alone: object vs object, array vs
/// array, array vs anything, and every combination involving a boxed `Mixed`.
///
/// Concrete operands are boxed into temporary `Mixed` cells first (the same
/// protocol `emit_mixed_strict_compare` uses) and released afterwards; operands
/// that already are `Mixed` are passed straight through and stay borrowed.
pub(super) fn emit_mixed_loose_compare(
    ctx: &mut FunctionContext<'_>,
    lhs: ValueId,
    lhs_ty: &PhpType,
    rhs: ValueId,
    rhs_ty: &PhpType,
    is_equal: bool,
) -> Result<()> {
    let left_ref = mixed_ref_cell_slot(ctx, lhs, lhs_ty)?;
    let right_ref = mixed_ref_cell_slot(ctx, rhs, rhs_ty)?;
    let comparator = if left_ref.is_some() || right_ref.is_some() {
        "__rt_mixed_loose_eq_refs"
    } else {
        "__rt_mixed_loose_eq"
    };
    let left_box_temp = !is_mixed_like(lhs_ty);
    let right_box_temp = !is_mixed_like(rhs_ty);
    let guard_size = crate::codegen_support::try_handlers::EXCEPTION_GUARD_SLOT_SIZE;
    let guard_bytes = (usize::from(left_box_temp) + usize::from(right_box_temp)) * guard_size;
    abi::emit_reserve_temporary_stack(ctx.emitter, guard_bytes);
    materialize_value_as_mixed(ctx, lhs, lhs_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if left_box_temp {
        super::super::callable_guards::guard_saved_owner(ctx.emitter, 16, 0);
    }
    materialize_value_as_mixed(ctx, rhs, rhs_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if right_box_temp {
        let record = 32 + usize::from(left_box_temp) * guard_size;
        super::super::callable_guards::guard_saved_owner(ctx.emitter, record, 0);
    }
    if left_ref.is_some() || right_ref.is_some() {
        materialize_ref_cell_or_null(ctx, left_ref)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        materialize_ref_cell_or_null(ctx, right_ref)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_reg_move(ctx.emitter, "x3", "x0");
                abi::emit_pop_reg(ctx.emitter, "x2");
            }
            Arch::X86_64 => {
                abi::emit_reg_move(ctx.emitter, "rcx", "rax");
                abi::emit_pop_reg(ctx.emitter, "rdx");
            }
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_call_label(ctx.emitter, comparator);
            if !is_equal {
                ctx.emitter.instruction("eor x0, x0, #1");                      // invert the mixed loose-equality result for !=
            }
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 0);
            abi::emit_call_label(ctx.emitter, comparator);
            if !is_equal {
                ctx.emitter.instruction("xor rax, 1");                          // invert the mixed loose-equality result for !=
            }
        }
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if left_box_temp {
        super::super::callable_guards::unguard_saved_owner(ctx.emitter, 48);
        decref_mixed_temp_at(ctx, 32);
    }
    if right_box_temp {
        let record = 48 + usize::from(left_box_temp) * guard_size;
        super::super::callable_guards::unguard_saved_owner(ctx.emitter, record);
        decref_mixed_temp_at(ctx, 16);
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_release_temporary_stack(ctx.emitter, 32 + guard_bytes);
    Ok(())
}

/// Finds reference storage that contains a boxed Mixed pointer, following ownership-only views.
fn mixed_ref_cell_slot(
    ctx: &FunctionContext<'_>,
    mut value: ValueId,
    ty: &PhpType,
) -> Result<Option<LocalSlotId>> {
    if !is_mixed_like(ty) {
        return Ok(None);
    }
    loop {
        let Some(inst) = super::super::local_stores::instruction_for_value(ctx, value)? else {
            return Ok(None);
        };
        if matches!(inst.op, Op::Acquire | Op::Move | Op::Borrow) {
            let Some(source) = inst.operands.first().copied() else {
                return Ok(None);
            };
            value = source;
            continue;
        }
        if inst.op != Op::LoadRefCell {
            return Ok(None);
        }
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            return Err(CodegenIrError::invalid_module("comparison ref-cell load has no local slot"));
        };
        return Ok(is_mixed_like(&ctx.local_mixed_box_type(slot)?).then_some(slot));
    }
}

/// Materializes a stable reference storage address, or zero for a value operand.
fn materialize_ref_cell_or_null(
    ctx: &mut FunctionContext<'_>,
    slot: Option<LocalSlotId>,
) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    if let Some(slot) = slot {
        ctx.materialize_local_storage_address(slot, result)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, result, 0);
    }
    Ok(())
}
