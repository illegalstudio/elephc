//! Purpose:
//! Activates the current source call's pending reference argument owners for native entry.
//!
//! Called from:
//! - Direct native calls after outgoing arguments and stack padding are materialized.
//!
//! Key details:
//! - Only explicit EIR argument-owner roles participate; ordinary PHP variables keep their owners.
//! - The incoming native SP separates pending, nested and recursive call activations.
//! - The leaf expansion preserves every outgoing argument register and never invokes PHP.

use super::*;
use crate::ir::{LocalSlotId, ValueId};
use std::collections::BTreeSet;

/// Associates the call's exact synthetic owner records with its final native entry stack pointer.
pub(super) fn activate(
    ctx: &mut FunctionContext<'_>, operands: &[ValueId], references: &[bool],
) -> Result<()> {
    let mut owners = BTreeSet::new();
    for (value, by_ref) in operands.iter().zip(references) {
        if !by_ref { continue; }
        let Ok(slot) = super::reference_arguments::local_slot_for_loaded_value(ctx, *value) else { continue; };
        let Some(owner) = ctx.function.locals[slot.as_raw() as usize].ref_cell_owner else { continue; };
        if ctx.function.locals[owner.as_raw() as usize].is_reference_argument_owner {
            owners.insert(owner);
        }
    }
    for owner in owners { activate_owner(ctx, owner)?; }
    Ok(())
}

/// Finds one owner-slot record without changing native argument registers or moving the stack.
fn activate_owner(ctx: &mut FunctionContext<'_>, owner: LocalSlotId) -> Result<()> {
    let scan = ctx.next_label("reference_argument_activate_scan");
    let found = ctx.next_label("reference_argument_activate_found");
    let done = ctx.next_label("reference_argument_activate_done");
    let (cursor, address, scratch) = if ctx.emitter.target.arch == Arch::AArch64 {
        ("x10", "x11", "x9")
    } else { ("r10", "rax", "r11") };
    let offset = ctx.local_offset(owner)?;
    abi::emit_load_symbol_to_reg(ctx.emitter, cursor, "_exc_call_frame_top", 0);
    abi::emit_frame_slot_address(ctx.emitter, address, offset);
    ctx.emitter.label(&scan);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbz {cursor}, {done}")),// older call surfaces keep their independent parameter retain
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {cursor}, {cursor}"));       // inspect only the activation cursor
            ctx.emitter.instruction(&format!("jz {done}"));                     // older call surfaces keep their independent parameter retain
        }
    }
    abi::emit_load_from_address(ctx.emitter, scratch, cursor, 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {scratch}, {address}"));      // select the exact owning slot for this argument
            ctx.emitter.instruction(&format!("b.eq {found}"));                  // pending outer arguments have different owning slots
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {scratch}, {address}"));      // select the exact owning slot for this argument
            ctx.emitter.instruction(&format!("je {found}"));                    // pending outer arguments have different owning slots
        }
    }
    abi::emit_load_from_address(ctx.emitter, cursor, cursor, 0);
    abi::emit_jump(ctx.emitter, &scan);
    ctx.emitter.label(&found);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {scratch}, sp"));             // capture the callee's actual incoming native stack pointer
            ctx.emitter.instruction(&format!("str {scratch}, [{cursor}, #32]"));// activate this reference argument only for that call
        }
        Arch::X86_64 => ctx.emitter.instruction(&format!("mov QWORD PTR [{cursor} + 32], rsp")),// activate this reference argument only for that call
    }
    ctx.emitter.label(&done);
    Ok(())
}
