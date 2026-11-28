//! Purpose:
//! Protects the two owned temporaries of a Mixed string-offset write.
//!
//! Called from:
//! - The runtime emitter around string-offset writes that may invoke a warning handler.
//!
//! Key details:
//! - The guard retires a cast buffer and any retained cell when the handler throws.
//! - Normal retirement returns both owners to the setter and preserves the result pair.

use crate::{abi, Arch, Emitter};

/// Aligned stack reservation for the activation prefix and its two payloads.
pub const STRING_WRITE_GUARD_BYTES: usize = 48;

/// Locations of the owner's temporaries in the outer setter frame.
/// AArch64 offsets start at the old stack pointer, x86_64 offsets are below the frame pointer.
pub struct OffsetWriteGuardSlots {
    /// Receiver cell pointer.
    pub cell: usize,
    /// Nonzero when the setter holds a receiver lease.
    pub retained: usize,
    /// Cast value's string pointer, nullable when there is no buffer to retire.
    pub cast: usize,
}

/// Links a guard that owns both temporaries while the string helper can unwind.
pub fn emit_link(emitter: &mut impl Emitter, slots: OffsetWriteGuardSlots) {
    match emitter.arch() {
        Arch::AArch64 => emit_aarch64(emitter, slots),
        Arch::X86_64 => emit_x86_64(emitter, slots),
    }
}

/// Returns both owners to the normal setter path and keeps its string result live.
/// On x86_64 the caller supplies the frame's two result spill offsets.
pub fn emit_unlink(emitter: &mut impl Emitter, pointer: usize, length: usize) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("mov x0, sp");                                  // the guard record, which the helper returned past
            emitter.instruction("bl __rt_exception_unguard_owned");             // unlink it; x1/x2 survive the call
            emitter.instruction(&format!("add sp, sp, #{STRING_WRITE_GUARD_BYTES}")); // pop the guard record
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rsp");                                // the guard record, which the helper returned past
            emitter.instruction(&format!("mov QWORD PTR [rbp - {pointer}], rax")); // keep the result pointer across the unlink
            emitter.instruction(&format!("mov QWORD PTR [rbp - {length}], rdx")); // keep the result length across the unlink
            emitter.instruction("call __rt_exception_unguard_owned");           // unlink the record
            emitter.instruction(&format!("add rsp, {STRING_WRITE_GUARD_BYTES}")); // pop the guard record
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {pointer}]")); // reload the result pointer
            emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {length}]")); // reload the result length
        }
    }
}

/// Pushes an AArch64 unwind guard for the string write, 48 bytes below the setter's frame.
///
/// `__rt_str_offset_set` can warn, and a user error handler that throws unwinds straight past
/// this frame, so its normal-path releases never run. The record (`[sp]` link, `[sp, #8]`
/// cleanup, `[sp, #16]` data, `[sp, #24]` the retained cell or 0, `[sp, #32]` the cast value)
/// hands them to `__rt_mixed_array_set_string_unwind` instead. Every frame slot is `#48`
/// further from `sp` until the record is popped.
fn emit_aarch64(emitter: &mut impl Emitter, slots: OffsetWriteGuardSlots) {
    emitter.instruction(&format!("sub sp, sp, #{STRING_WRITE_GUARD_BYTES}"));   // reserve the guard record below the frame
    abi::emit_symbol_address(emitter, "x9", "__rt_mixed_array_set_string_unwind");
    emitter.instruction("str x9, [sp, #8]");                                    // the cleanup the unwinder calls
    emitter.instruction(&format!("ldr x9, [sp, #{}]", STRING_WRITE_GUARD_BYTES + slots.retained)); // the retained-cell flag
    emitter.instruction(&format!("ldr x10, [sp, #{}]", STRING_WRITE_GUARD_BYTES + slots.cell)); // the target Mixed cell
    emitter.instruction("cmp x9, #0");                                          // was the cell retained?
    emitter.instruction("csel x10, x10, xzr, ne");                              // only a retained cell is ours to give back
    emitter.instruction("str x10, [sp, #24]");                                  // the cell owner, or 0
    emitter.instruction(&format!("ldr x9, [sp, #{}]", STRING_WRITE_GUARD_BYTES + slots.cast)); // the cast value pointer (0 once released)
    emitter.instruction("str x9, [sp, #32]");                                   // the cast value owner
    emitter.instruction("mov x0, sp");                                          // the record
    emitter.instruction("mov x1, #0");                                          // link it at the chain head
    emitter.instruction("bl __rt_exception_guard_owned");                       // publish it for the unwinder
}

/// Pushes the x86_64 unwind guard for the string write; see the AArch64 variant. Frame slots
/// are `rbp`-relative, so they keep their offsets while the record sits at `[rsp]`.
fn emit_x86_64(emitter: &mut impl Emitter, slots: OffsetWriteGuardSlots) {
    emitter.instruction(&format!("sub rsp, {STRING_WRITE_GUARD_BYTES}"));       // reserve the guard record, keeping calls aligned
    emitter.instruction("lea r10, [rip + __rt_mixed_array_set_string_unwind]"); // the cleanup the unwinder calls
    emitter.instruction("mov QWORD PTR [rsp + 8], r10");                        // store it in the record
    emitter.instruction("xor r10d, r10d");                                      // no cell owner unless it was retained
    emitter.instruction(&format!("cmp QWORD PTR [rbp - {}], 0", slots.retained)); // was the cell retained?
    emitter.instruction(&format!("cmovne r10, QWORD PTR [rbp - {}]", slots.cell)); // then the target cell is ours to give back
    emitter.instruction("mov QWORD PTR [rsp + 24], r10");                       // the cell owner, or 0
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", slots.cast)); // the cast value pointer (0 once released)
    emitter.instruction("mov QWORD PTR [rsp + 32], r10");                       // the cast value owner
    emitter.instruction("mov rdi, rsp");                                        // the record
    emitter.instruction("xor esi, esi");                                        // link it at the chain head
    emitter.instruction("call __rt_exception_guard_owned");                     // publish it for the unwinder
}

/// Emits `__rt_mixed_array_set_string_unwind`, the cleanup the unwinder calls with the guard
/// record when a warning handler throws out of the string write: it frees the cast value and
/// gives back the retained cell reference, the two releases the normal path would have made.
pub fn emit_unwind(emitter: &mut impl Emitter) {
    emitter.label_global("__rt_mixed_array_set_string_unwind");
    if emitter.arch() == Arch::X86_64 {
        emitter.instruction("push rbx");                                        // keep the record in a callee-saved register
        emitter.instruction("mov rbx, rdi");                                    // the guard record
        emitter.instruction("mov rax, QWORD PTR [rbx + 32]");                   // the cast value
        emitter.instruction("call __rt_heap_free_safe");                        // free it (no-op for scratch or 0)
        emitter.instruction("mov rax, QWORD PTR [rbx + 24]");                   // the retained cell, or 0
        emitter.instruction("test rax, rax");                                   // was a reference retained?
        emitter.instruction("jz __rt_mixed_array_set_string_unwind_done");      // nothing to give back
        emitter.instruction("call __rt_decref_mixed");                          // give the retained reference back
        emitter.label("__rt_mixed_array_set_string_unwind_done");
        emitter.instruction("pop rbx");                                         // restore the callee-saved register
        emitter.instruction("ret");                                             // resume the unwinder
        return;
    }
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // save frame linkage
    emitter.instruction("mov x29, sp");                                         // establish the cleanup frame
    emitter.instruction("str x0, [sp, #16]");                                   // keep the guard record
    emitter.instruction("ldr x0, [x0, #32]");                                   // the cast value
    emitter.instruction("bl __rt_heap_free_safe");                              // free it (no-op for scratch or 0)
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the guard record
    emitter.instruction("ldr x0, [x9, #24]");                                   // the retained cell, or 0
    emitter.instruction("cbz x0, __rt_mixed_array_set_string_unwind_done");     // nothing to give back
    emitter.instruction("bl __rt_decref_mixed");                                // give the retained reference back
    emitter.label("__rt_mixed_array_set_string_unwind_done");
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame linkage
    emitter.instruction("ret");                                                 // resume the unwinder
}
