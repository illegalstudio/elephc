//! Purpose:
//! Boxes the integer key of a by-value array `foreach` into the cell its key variable already
//! owns.
//!
//! Called from:
//! - `super::lower_iter_current_key` and its dynamic-source dispatch when the `IterCurrentKey`
//!   instruction names the key variable's Mixed local slot.
//!
//! Key details:
//! - Every key used to allocate a fresh Mixed cell while the binding released the previous
//!   one, so a keyed loop cost one allocation per entry. `__rt_mixed_reuse_int_cell` rewrites
//!   the previous cell in place when the slot is its only owner and it holds a scalar.
//! - The helper clears the slot when it reuses the cell, so the instruction still returns an
//!   owned value and the binding's ordinary store finds nothing left to release.
//! - Only integer keys take this path. A string key of a hash keeps the allocating box,
//!   which persists the key bytes.
//! - A parameter slot or a slot without epilogue cleanup may hold a cell the frame does not
//!   own, and a generator or fiber keeps locals in resumable state, so those never lend a cell.

use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, Result};
use crate::ir::{Immediate, Instruction, IrHeapKind, IrType, LocalKind, LocalSlotId};

use super::{ITER_CURSOR_OFFSET_DELTA, ITER_KEY_HI_OFFSET_DELTA, ITER_KEY_LO_OFFSET_DELTA};

/// Returns the key variable's slot when its cell may be rewritten by this instruction.
pub(super) fn reusable_slot(ctx: &FunctionContext<'_>, inst: &Instruction) -> Option<LocalSlotId> {
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return None;
    };
    let function = ctx.function;
    // A suspended generator or fiber keeps its locals in resumable state, and a parameter or
    // a slot without epilogue cleanup may hold a cell the frame does not own.
    let frame_owned = !function.flags.is_generator
        && !function.flags.is_fiber_wrapper
        && (slot.as_raw() as usize) >= function.params.len()
        && !function.no_epilogue_cleanup_slots.contains(&slot)
        && function.locals.get(slot.as_raw() as usize).is_some_and(|local| {
            local.kind == LocalKind::PhpLocal && local.ir_type == IrType::Heap(IrHeapKind::Mixed)
        });
    frame_owned.then_some(slot)
}

/// Boxes the cursor of an indexed array, which is the integer key of the current element.
pub(super) fn lower_indexed(ctx: &mut FunctionContext<'_>, slot: LocalSlotId, offset: usize) -> Result<()> {
    let (_, key) = helper_regs(ctx);
    abi::load_at_offset(ctx.emitter, key, offset - ITER_CURSOR_OFFSET_DELTA);
    emit_reuse_call(ctx, slot)
}

/// Boxes the current hash key, reusing the slot's cell when the key is an integer.
pub(super) fn lower_hash(ctx: &mut FunctionContext<'_>, slot: LocalSlotId, offset: usize) -> Result<()> {
    let string_key = ctx.next_label("iter_hash_key_fresh");
    let done = ctx.next_label("iter_hash_key_reused");
    let (_, key) = helper_regs(ctx);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, scratch, offset - ITER_KEY_HI_OFFSET_DELTA);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmn {scratch}, #1"));             // is this normalized hash key integer-backed?
            ctx.emitter.instruction(&format!("b.ne {string_key}"));             // a string key keeps the allocating box that persists its bytes
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {scratch}, -1"));             // is this normalized hash key integer-backed?
            ctx.emitter.instruction(&format!("jne {string_key}"));              // a string key keeps the allocating box that persists its bytes
        }
    }
    abi::load_at_offset(ctx.emitter, key, offset - ITER_KEY_LO_OFFSET_DELTA);
    emit_reuse_call(ctx, slot)?;
    ctx.emit_branch(&done);
    ctx.emitter.label(&string_key);
    match ctx.emitter.target.arch {
        Arch::AArch64 => super::load_current_hash_key_as_mixed_aarch64(ctx, offset),
        Arch::X86_64 => super::load_current_hash_key_as_mixed_x86_64(ctx, offset),
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Passes the slot address beside the loaded key and leaves the owned cell in the result register.
fn emit_reuse_call(ctx: &mut FunctionContext<'_>, slot: LocalSlotId) -> Result<()> {
    let (address, _) = helper_regs(ctx);
    let slot_offset = ctx.local_offset(slot)?;
    abi::emit_frame_slot_address(ctx.emitter, address, slot_offset);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_reuse_int_cell");
    Ok(())
}

/// Returns the slot-address and integer-payload argument registers of the reuse helper.
fn helper_regs(ctx: &FunctionContext<'_>) -> (&'static str, &'static str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x1"),
        Arch::X86_64 => ("rdi", "rsi"),
    }
}
