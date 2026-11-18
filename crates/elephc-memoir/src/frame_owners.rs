//! Purpose:
//! Transfers an owner out of a frame slot before calling cleanup.
//!
//! Called from:
//! - Compiler emitters for array callbacks and owned callable arguments.
//!
//! Key details:
//! - Clearing the slot before destruction makes repeated cleanup safe.
//! - The caller supplies the release entry and its surrounding exception handling.

use crate::{abi, Arch, Emitter, FrameOffset, TargetAbi};

/// Clears a pointer-sized owner slot without disturbing the native result register.
pub fn emit_clear(emitter: &mut impl TargetAbi, offset: FrameOffset) {
    let scratch = emitter.secondary_scratch_reg();
    emitter.emit_load_int_immediate(scratch, 0);
    emitter.store_at_offset(scratch, offset.bytes());
}

/// Takes the owner and calls a unary native release entry under the caller's handler.
pub fn emit_release(emitter: &mut impl TargetAbi, offset: FrameOffset, entry: &str) {
    emit_take(emitter, offset);
    emitter.emit_call_label(entry);
}

/// Takes the owner before entering bounded cleanup with a caller-owned pending slot.
pub fn emit_guarded_release(
    emitter: &mut impl Emitter,
    offset: FrameOffset,
    entry: &str,
    pending: FrameOffset,
) {
    emit_take(emitter, offset);
    emitter.emit_guarded_cleanup_call(entry, emitter.int_result_reg(), pending.bytes());
}

/// Transfers the owner to the native result register and clears its former slot.
fn emit_take(emitter: &mut impl TargetAbi, offset: FrameOffset) {
    emitter.load_at_offset(emitter.int_result_reg(), offset.bytes());
    emit_clear(emitter, offset);
}

/// Preserves the caller's real stack while selecting a borrowed PHP frame for ordinary slot helpers.
pub fn enter_borrowed_frame(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction("sub sp, sp, #48");                                 // reserve cleanup state and saved linkage outside the borrowed PHP frame
        emitter.instruction("stp x29, x30, [sp, #32]");                         // preserve caller linkage across protected cleanup calls
        emitter.instruction("str x19, [sp, #16]");                              // retain the caller's callee-saved stack anchor
        emitter.instruction("mov x19, sp");                                     // remember this helper's actual stack independently of the borrowed frame
        emitter.instruction("mov x29, x0");                                     // address the abandoned frame's original owned local slots
    } else {
        emitter.instruction("push rbp");                                        // retain the caller's frame pointer before borrowing the abandoned frame
        emitter.instruction("push r12");                                        // preserve the register used to anchor this helper's real stack
        emitter.instruction("sub rsp, 24");                                     // align nested calls and reserve cleanup state outside the PHP frame
        emitter.instruction("mov r12, rsp");                                    // remember the real helper stack across slot-based cleanup calls
        emitter.instruction("mov rbp, rdi");                                    // address the abandoned frame's original owned local slots
    }
}

/// Restores actual helper storage and caller registers while preserving the cleanup result.
pub fn leave_borrowed_frame(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction("mov sp, x19");                                     // leave any borrowed-frame addressing behind before restoring linkage
        emitter.instruction("ldr x19, [sp, #16]");                              // restore the caller's stack-anchor register
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore the caller's frame and return address
        emitter.instruction("add sp, sp, #48");                                 // release only this helper's cleanup state
    } else {
        emitter.instruction("mov rsp, r12");                                    // recover this helper's real stack after borrowed-frame cleanup
        emitter.instruction("add rsp, 24");                                     // release aligned cleanup state
        emitter.instruction("pop r12");                                         // restore the caller's callee-saved stack anchor
        emitter.instruction("pop rbp");                                         // restore the caller's frame pointer
    }
    emitter.instruction("ret");                                                 // return without propagating a pending exception through the cleanup walker
}

/// Removes a present reference owner from the borrowed frame, branching past an empty slot.
pub fn emit_take_reference_owner(emitter: &mut impl TargetAbi, offset: FrameOffset, empty: &str) {
    let result = emitter.int_result_reg();
    emitter.load_at_offset(result, offset.bytes());
    emitter.emit_branch_if_int_result_zero(empty);
    emitter.emit_store_zero_to_local_slot(offset.bytes());
}

/// Releases a detached raw reference cell from a borrowed PHP frame.
/// The aligned private stack starts with the cell and its own pending exception flag.
pub fn emit_release_borrowed_raw_ref_cell(
    emitter: &mut impl Emitter,
    payload_entry: Option<&str>,
    done: &str,
) {
    let arm = emitter.arch() == Arch::AArch64;
    emitter.instruction(if arm { "str x0, [sp]" } else { "mov QWORD PTR [rsp], rax" }); // retain the raw cell owner until its payload cleanup finishes
    emitter.instruction(if arm { "str xzr, [sp, #8]" } else { "mov QWORD PTR [rsp + 8], 0" }); // keep this cell's pending flag separate from the enclosing owner walk
    if let Some(payload_entry) = payload_entry {
        emitter.instruction(if arm { "mov x1, x0" } else { "mov rsi, rax" });   // borrow the cell only while the payload step reads its owned value
        abi::emit_symbol_address(emitter, if arm { "x0" } else { "rdi" }, payload_entry);
        emitter.instruction(if arm { "add x2, sp, #8" } else { "lea rdx, [rsp + 8]" }); // contain a payload exception without abandoning the raw cell
        abi::emit_call_label(emitter, "__rt_cleanup_call");
    }
    emitter.instruction(if arm { "ldr x0, [sp]" } else { "mov rax, QWORD PTR [rsp]" }); // recover the raw cell after its payload owner was consumed
    abi::emit_call_label(emitter, "__rt_heap_free");
    emitter.instruction(if arm { "ldr x0, [sp, #8]" } else { "mov rax, QWORD PTR [rsp + 8]" }); // propagate payload failure only after the cell storage is freed
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_call_label(emitter, "__rt_throw_current");
}

/// Loads the raw cell's payload for the compiler-selected release operation.
pub fn emit_raw_ref_payload_callback<E: Emitter>(
    emitter: &mut E,
    symbol: &str,
    release: impl FnOnce(&mut E),
) {
    emitter.label(symbol);
    let arm = emitter.arch() == Arch::AArch64;
    emitter.instruction(if arm { "stp x29, x30, [sp, #-16]!" } else { "sub rsp, 8" }); // preserve linkage and align payload cleanup calls
    let result = abi::int_result_reg(emitter);
    abi::emit_load_from_address(emitter, result, result, 0);
    release(emitter);
    emitter.instruction(if arm { "ldp x29, x30, [sp], #16" } else { "add rsp, 8" }); // restore linkage after successful payload cleanup
    emitter.instruction("ret");                                                 // transfer any exceptional exit to the enclosing protected caller
}
