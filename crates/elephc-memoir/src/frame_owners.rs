//! Purpose:
//! Transfers an owner out of a frame slot before calling cleanup.
//!
//! Called from:
//! - Compiler emitters for array callbacks and owned callable arguments.
//!
//! Key details:
//! - Clearing the slot before destruction makes repeated cleanup safe.
//! - The caller supplies the release entry and its surrounding exception handling.

use crate::{abi, Arch, Emitter, FrameOffset, RecordOffset, TargetAbi};

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
    crate::cleanup::invoke::emit_guarded_cleanup_call(emitter, entry, emitter.int_result_reg(), pending.bytes());
}

/// Transfers the owner to the native result register and clears its former slot.
fn emit_take(emitter: &mut impl TargetAbi, offset: FrameOffset) {
    emitter.load_at_offset(emitter.int_result_reg(), offset.bytes());
    emit_clear(emitter, offset);
}

/// Transfers an owner from an addressed native record into the result register.
pub fn emit_take_addressed(emitter: &mut impl TargetAbi, record: &str, offset: RecordOffset) {
    let result = emitter.int_result_reg();
    emitter.emit_load_from_address(result, record, offset.bytes());
    emitter.emit_store_zero_to_address(record, offset.bytes());
}

/// Transfers a stack-resident owner and clears its slot before retirement.
pub fn emit_take_stack(emitter: &mut impl Emitter, offset: RecordOffset) {
    let offset = offset.bytes();
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [sp, #{offset}]"));           // acquire the former owner for cleanup
            emitter.instruction(&format!("str xzr, [sp, #{offset}]"));          // prevent repeated release when cleanup is resumed
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {offset}]")); // acquire the former owner for cleanup
            emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], 0")); // prevent repeated release when cleanup is resumed
        }
    }
}

/// Retires typed owner slots from a borrowed record passed as the first native argument.
pub fn emit_addressed_cleanup(
    emitter: &mut impl Emitter,
    label: &str,
    owners: &[(usize, &str)],
) {
    emitter.label_global(label);
    abi::emit_frame_prologue(emitter, 32);
    emitter.emit_store_to_sp(emitter.int_arg_reg_name(0), 0);
    let record = emitter.secondary_scratch_reg();
    for &(offset, entry) in owners {
        emitter.emit_load_temporary_stack_slot(record, 0);
        emit_take_addressed(emitter, record, RecordOffset::new(offset));
        emitter.emit_call_label(entry);
    }
    abi::emit_frame_restore(emitter, 32);
    emitter.instruction("ret");                                                 // return after retiring each detached owner
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

/// Invokes the caller's protected cleanup steps while the borrowed PHP frame remains readable.
pub fn emit_cleanup_steps(emitter: &mut impl Emitter, callback: &str, count: usize) {
    let arm = emitter.arch() == Arch::AArch64;
    emitter.instruction(if arm { "str xzr, [sp]" } else { "mov QWORD PTR [rsp], 0" }); // accumulate child failures while continuing the cleanup walk
    for index in 0..count {
        let step = format!("{callback}_owner_{index}");
        abi::emit_symbol_address(emitter, if arm { "x0" } else { "rdi" }, &step);
        emitter.instruction(if arm { "mov x1, x29" } else { "mov rsi, rbp" }); // pass the still-readable original frame to this cleanup step
        emitter.instruction(if arm { "mov x2, sp" } else { "mov rdx, rsp" }); // retain pending status in this walker's own stack storage
        abi::emit_call_label(emitter, "__rt_cleanup_call");
    }
    emitter.instruction(if arm { "ldr x0, [sp]" } else { "mov rax, QWORD PTR [rsp]" }); // report whether any owner cleanup produced a throwable
    emitter.instruction(if arm { "lsl x0, x0, #1" } else { "shl eax, 1" }); // convert the cleanup flag to the shared pending status
}

/// Saves the suspended-frame visitor's independent native context on its private stack.
pub fn emit_save_visit_context(emitter: &mut impl Emitter) {
    let arm = emitter.arch() == Arch::AArch64;
    emitter.instruction(if arm { "str x1, [sp, #8]" } else { "mov QWORD PTR [rsp + 8], rsi" }); // retain this traversal's independent native context
}

/// Reloads the pending visitor context as the third argument of a pure slot visit.
pub fn emit_load_visit_context(emitter: &mut impl Emitter) {
    emitter.instruction(if emitter.arch() == Arch::AArch64 {
        "ldr x2, [sp, #8]"
    } else { "mov rdx, QWORD PTR [rsp + 8]" });                                  // restore the visitor's private context
}

/// Passes the real slot address, its normalization tag and the suspended-frame visitor context.
pub fn emit_visit_frame_slot(emitter: &mut impl Emitter, offset: usize, tag: i64) {
    emitter.emit_frame_slot_address(emitter.int_arg_reg_name(0), offset);
    emitter.emit_load_int_immediate(emitter.int_arg_reg_name(1), tag);
    emit_load_visit_context(emitter);
    emitter.emit_call_label("__rt_gc_frame_owner_slot");
}

/// Releases a temporary Mixed lease while preserving the caller's scalar result.
pub fn emit_release_stacked_mixed_preserving_result(emitter: &mut impl TargetAbi, offset: usize) {
    let result = emitter.int_result_reg();
    emitter.emit_push_reg(result);
    emitter.emit_load_temporary_stack_slot(result, offset + 16);
    emitter.emit_call_label("__rt_decref_mixed");
    emitter.emit_pop_reg(result);
}
