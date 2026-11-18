//! Purpose:
//! Publishes and queries stack-backed borrowed reference slots during native callbacks.
//!
//! Called from:
//! - Array-walk callbacks, native reference escape checks and arena reset.
//!
//! Key details:
//! - Each record holds a previous node followed by the borrowed slot address.
//! - Restoring the saved head detaches records on both normal return and unwind.

use crate::{Arch, Emitter, FrameOffset, TargetAbi, abi};

/// Runtime head of the active borrowed reference records.
pub const HEAD_SYMBOL: &str = "_rt_unmanaged_ref_borrow_top";

/// Saves the current borrow chain in a frame slot for later restoration.
pub fn emit_save_head(emitter: &mut impl TargetAbi, register: &str, saved: FrameOffset) {
    abi::emit_load_symbol_to_reg(emitter, register, HEAD_SYMBOL, 0);
    abi::store_at_offset(emitter, register, saved.bytes());
}

/// Links a two-word frame record containing one borrowed reference address.
pub fn emit_push(
    emitter: &mut impl TargetAbi,
    scratch: &str,
    record: FrameOffset,
    source: FrameOffset,
) {
    abi::emit_load_symbol_to_reg(emitter, scratch, HEAD_SYMBOL, 0);
    abi::store_at_offset(emitter, scratch, record.bytes());
    abi::load_at_offset(emitter, scratch, source.bytes());
    abi::store_at_offset(emitter, scratch, record.bytes() - 8);
    abi::emit_frame_slot_address(emitter, scratch, record.bytes());
    abi::emit_store_reg_to_symbol(emitter, scratch, HEAD_SYMBOL, 0);
}

/// Restores the head saved by a visitor or an enclosing callback boundary.
pub fn emit_restore_head(emitter: &mut impl TargetAbi, scratch: &str, saved: FrameOffset) {
    abi::load_at_offset(emitter, scratch, saved.bytes());
    abi::emit_store_reg_to_symbol(emitter, scratch, HEAD_SYMBOL, 0);
}

/// Reports whether the supplied address is an active boxed-array-walk element borrow.
pub fn emit_unmanaged_borrow_lookup(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.label_global("__rt_reference_cell_is_unmanaged_borrow");
    match emitter.arch() {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", HEAD_SYMBOL, 0);
            emitter.label("__rt_reference_cell_unmanaged_borrow_scan");
            emitter.instruction("cbz x9, __rt_reference_cell_unmanaged_borrow_none"); // an empty chain cannot contain the candidate address
            emitter.instruction("ldr x10, [x9, #8]");                           // read this active node's borrowed hash-entry slot
            emitter.instruction("cmp x0, x10");                                 // require the exact exposed cell, not another array-interior address
            emitter.instruction("b.eq __rt_reference_cell_unmanaged_borrow_found"); // report an active unmanaged borrow to the escape boundary
            emitter.instruction("ldr x9, [x9]");                                // continue through nested and reentrant boxed walks
            emitter.instruction("b __rt_reference_cell_unmanaged_borrow_scan"); // scan the complete stack-backed borrow chain
            emitter.label("__rt_reference_cell_unmanaged_borrow_found");
            emitter.instruction("mov x0, #1");                                  // boolean true: this reference may not be published
            abi::emit_return(emitter);
            emitter.label("__rt_reference_cell_unmanaged_borrow_none");
            emitter.instruction("mov x0, xzr");                                 // boolean false: ordinary references retain their existing behavior
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", HEAD_SYMBOL, 0);
            emitter.label("__rt_reference_cell_unmanaged_borrow_scan");
            emitter.instruction("test r10, r10");                               // an empty chain cannot contain the candidate address
            emitter.instruction("jz __rt_reference_cell_unmanaged_borrow_none"); // leave ordinary references accepted
            emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                // read this active node's borrowed hash-entry slot
            emitter.instruction("cmp rax, r11");                                // require the exact exposed cell, not another array-interior address
            emitter.instruction("je __rt_reference_cell_unmanaged_borrow_found"); // report an active unmanaged borrow to the escape boundary
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // continue through nested and reentrant boxed walks
            emitter.instruction("jmp __rt_reference_cell_unmanaged_borrow_scan"); // scan the complete stack-backed borrow chain
            emitter.label("__rt_reference_cell_unmanaged_borrow_found");
            emitter.instruction("mov eax, 1");                                  // boolean true: this reference may not be published
            abi::emit_return(emitter);
            emitter.label("__rt_reference_cell_unmanaged_borrow_none");
            emitter.instruction("xor eax, eax");                                // boolean false: ordinary references retain their existing behavior
        }
    }
    abi::emit_return(emitter);
}
