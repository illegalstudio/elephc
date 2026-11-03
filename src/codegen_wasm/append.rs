//! Purpose:
//! Fuses a string local's self-append (`$x .= e`, `$x = $x . e`) into one in-place
//! `__rt_str_append`, so building a string in a loop is amortized linear instead of copying the
//! whole string, twice, on every iteration.
//!
//! Called from:
//! - `crate::codegen_wasm::function::emit_dispatch_loop`, once per segment, before the
//!   segment's instructions are lowered.
//!
//! Key details:
//! - The EIR spells a self-append as `load_local S; str_concat; acquire; release_local_slot S;
//!   store_local S; release` (rhs temporaries may be released in between). Lowered literally that
//!   is a fresh block for the concat, a second block for the `acquire`'s copy, and a free of the
//!   old string: two allocations and two full copies per iteration, and, because the allocator
//!   neither splits nor coalesces, blocks that never fit the next, longer string.
//! - The window is fused only when every piece is exactly that shape, the concat result and the
//!   acquired copy have no other reader, and the slot is a plain frame local holding a string (not a
//!   ref-cell, not a by-value closure capture that still borrows the descriptor's value). The
//!   fused code reads the slot's CURRENT value at the append (php evaluates the right-hand side
//!   first) and writes the result straight back.
//! - Soundness of growing in place rests on `release_local_slot S` already treating the slot's
//!   string as owned: the unfused code frees that block, so extending it instead cannot reach
//!   anything the free did not. `__rt_str_append` still refuses any block that is not live, kind 1
//!   and single-owner, so literals and views fall back to a fresh block.

use super::context::{FnCtx, Result};
use super::values::WasmRepr;
use crate::ir::{Immediate, InstId, IrType, LocalSlotId, Op, ValueDef, ValueId};
use std::collections::{HashMap, HashSet};

/// One fused self-append: the concat to replace and the instructions it makes redundant.
pub(super) struct AppendPlan {
    /// The slot appended to and written back.
    pub(super) slot: LocalSlotId,
    /// The `str_concat` operand that is the right-hand side.
    pub(super) rhs: ValueId,
}

/// The fusion decisions for one segment.
#[derive(Default)]
pub(super) struct AppendPlans {
    /// Fused concats, keyed by the concat instruction.
    pub(super) fused: HashMap<InstId, AppendPlan>,
    /// Instructions that the fused append subsumes and lowering must skip.
    pub(super) skipped: HashSet<InstId>,
}

/// How far past a concat the matching store may sit.
const WINDOW: usize = 14;

/// Finds every fusable self-append in one segment's instruction list.
pub(super) fn plan_appends(ctx: &FnCtx, ids: &[InstId]) -> AppendPlans {
    let mut plans = AppendPlans::default();    for (pos, id) in ids.iter().enumerate() {
        let Some(inst) = ctx.function.instruction(*id) else { continue };
        if inst.op != Op::StrConcat || inst.operands.len() != 2 {
            continue;
        }
        if let Some((plan, skip)) = plan_one(ctx, ids, pos) {
            plans.fused.insert(*id, plan);
            plans.skipped.extend(skip);
        }
    }
    plans
}

/// The slot a `load_local` instruction reads, when `value` is defined by one.
fn loaded_slot(ctx: &FnCtx, value: ValueId) -> Option<LocalSlotId> {
    let def = ctx.function.value(value)?;
    let ValueDef::Instruction { inst, .. } = def.def else { return None };
    let inst = ctx.function.instruction(inst)?;
    match (inst.op, &inst.immediate) {
        (Op::LoadLocal, Some(Immediate::LocalSlot(slot))) => Some(*slot),
        _ => None,
    }
}

/// Matches the self-append window that starts at `ids[pos]`.
fn plan_one(ctx: &FnCtx, ids: &[InstId], pos: usize) -> Option<(AppendPlan, Vec<InstId>)> {
    let concat = ctx.function.instruction(ids[pos])?;
    let result = concat.result?;
    let lhs = concat.operands[0];
    let rhs = concat.operands[1];
    let slot = loaded_slot(ctx, lhs)?;
    if !slot_is_appendable(ctx, slot) {
        return None;
    }
    let users = &ctx.usage().user_results;
    // The loaded string feeds only this concat, and the concat feeds only the acquire + release.
    if users.get(&lhs).map(Vec::len) != Some(1) || users.get(&result).map(Vec::len) != Some(2) {
        return None;
    }
    let mut skip = Vec::new();
    let (mut acquire, mut acquired, mut released_slot, mut stored, mut released) =
        (false, None, false, false, false);
    for id in ids.iter().skip(pos + 1).take(WINDOW) {
        let inst = ctx.function.instruction(*id)?;
        match inst.op {
            Op::Nop => {}
            Op::Acquire if !acquire && inst.operands == [result] => {
                acquire = true;
                acquired = inst.result;
                skip.push(*id);
            }
            Op::ReleaseLocalSlot
                if acquire
                    && !released_slot
                    && inst.immediate == Some(Immediate::LocalSlot(slot)) =>
            {
                released_slot = true;
                skip.push(*id);
            }
            Op::StoreLocal
                if released_slot
                    && !stored
                    && inst.immediate == Some(Immediate::LocalSlot(slot))
                    && inst.operands.len() == 1
                    && Some(inst.operands[0]) == acquired =>
            {
                stored = true;
                skip.push(*id);
            }
            Op::Release if inst.operands == [result] && stored && !released => {
                released = true;
                skip.push(*id);
            }
            // A temporary the right-hand side built, released after the concat read it. Only a
            // STRING may sit here: the fused append publishes the new value at the concat, so a
            // release that could run a destructor and throw would let a `catch` see the slot
            // already updated, where the unfused assignment had not happened yet.
            Op::Release if inst.operands.len() == 1 && inst.operands[0] != result => {
                let is_string = ctx
                    .function
                    .value(inst.operands[0])
                    .is_some_and(|value| value.ir_type == IrType::Str);
                if Some(inst.operands[0]) == acquired || !is_string {
                    return None;
                }
            }
            _ => return None,
        }
        if released {
            break;
        }
    }
    if !(acquire && released_slot && stored && released) {
        return None;
    }
    // The acquired copy has exactly one reader: the store.
    let acquired = acquired?;
    if users.get(&acquired).map(Vec::len) != Some(1) {
        return None;
    }
    Some((AppendPlan { slot, rhs }, skip))
}

/// Whether `slot` is a plain frame local the fused append may read and rewrite directly.
fn slot_is_appendable(ctx: &FnCtx, slot: LocalSlotId) -> bool {
    if ctx.ref_cell_ptr(slot.as_raw()).is_ok() {
        return false;
    }
    if ctx.capture_owned_flags.contains_key(&slot.as_raw()) {
        return false;
    }
    // A by-value parameter's slot holds the CALLER's string until the body first stores to it,
    // so growing that block in place would write into the caller's string.
    if (slot.as_raw() as usize) < ctx.function.params.len() {
        return false;
    }
    let Some(local) = ctx.function.locals.get(slot.as_raw() as usize) else {
        return false;
    };
    if super::inst::capture_slot_still_borrows_from_descriptor(
        ctx,
        slot,
        &local.php_type.codegen_repr(),
    ) {
        return false;
    }
    matches!(ctx.slot_repr(slot), Ok(WasmRepr::Str { .. }))
}

/// Emits the fused append: reads the slot's string, appends the right-hand side, and writes the
/// result back to the slot.
pub(super) fn emit_append(ctx: &mut FnCtx, plan: &AppendPlan) -> Result<()> {
    let WasmRepr::Str { ptr, len } = ctx.slot_repr(plan.slot)?.clone() else {
        return Err(super::WasmError::Unsupported(
            "fused string append on a non-string slot".to_string(),
        ));
    };
    ctx.fb.ins(&format!("local.get {ptr}"), "the slot's current string");
    ctx.fb.ins(&format!("local.get {len}"), "its length");
    ctx.emit_load_value(plan.rhs)?;
    ctx.fb.ins(
        "call $__rt_str_append",
        "append in place when the slot's block has room, else grow geometrically",
    );
    ctx.fb.ins(&format!("local.set {len}"), "the slot's new length");
    ctx.fb.ins(&format!("local.set {ptr}"), "the slot's new string");
    Ok(())
}
