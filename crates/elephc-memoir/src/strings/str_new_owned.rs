//! Purpose:
//! Emits `__rt_str_new_owned`, the direct owned-heap string allocation used by string
//! transforms that can write their result exactly once.
//!
//! Called from:
//! - `elephc::codegen_support::runtime::emitters::emit_runtime()` via
//!   `elephc::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Input: `x0` (AArch64) / `rax` (x86_64) = payload byte count.
//! - Output: `x0` / `rax` = payload pointer of a heap block stamped as heap kind 1
//!   (owned elephc string), ready to be filled by the caller.
//! - Requests above `_heap_max` take the same PHP allocation-overflow fatal as
//!   `__rt_concat_reserve`; the allocator rounds the payload to eight bytes.
//! - Callers must save their live state first: the allocator clobbers caller-saved
//!   registers on both targets.

use crate::layout::STRING_HEAP_KIND;
use crate::{abi, Arch, Emitter, HeapKind};

/// Replaces a work allocation with a larger one, preserving its initialized prefix.
///
/// Inputs use sp/rsp-relative spills and the requested capacity in the native result.
/// Returns the replacement in x9/r9 and installs it in the owner slot.
pub fn emit_replace_work_buffer(
    emitter: &mut impl Emitter, owner: usize, length: usize, replacement: usize,
) {
    emit_allocate_owned_string(emitter);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("str x0, [sp, #{replacement}]"));      // park the new owner until the copy and release complete
            emitter.instruction(&format!("ldr x1, [sp, #{owner}]"));            // load the old data pointer
            emitter.instruction(&format!("ldr x2, [sp, #{length}]"));           // copy only initialized bytes
            crate::strings::strcopy::emit_copy_bytes(emitter);
            emitter.instruction(&format!("ldr x0, [sp, #{owner}]"));            // retire the allocation superseded by the copy
            emitter.emit_call_label("__rt_heap_free_safe");
            emitter.instruction(&format!("ldr x9, [sp, #{replacement}]"));      // recover the surviving owner
            emitter.instruction(&format!("str x9, [sp, #{owner}]"));            // publish the replacement to the caller
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [rsp + {replacement}], rax")); // park the new owner across copy and release
            emitter.instruction("mov rdi, rax");                                // pass the new data buffer as the copy destination
            emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {owner}]")); // load the old data pointer
            emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {length}]")); // copy only initialized bytes
            crate::strings::strcopy::emit_copy_bytes(emitter);
            emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {owner}]")); // retire the allocation superseded by the copy
            emitter.emit_call_label("__rt_heap_free_safe");
            emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {replacement}]")); // recover the surviving owner
            emitter.instruction(&format!("mov QWORD PTR [rsp + {owner}], r9")); // publish the replacement to the caller
        }
    }
}

/// Emits an inline owned-string allocation with the byte count supplied by the caller.
pub fn emit_allocate_owned_string(emitter: &mut impl Emitter) {
    crate::heap_alloc::emit_fresh_with_kind(emitter, HeapKind::new(STRING_HEAP_KIND));
}

/// Grows a prepared concat buffer and marks the result as an independent string owner.
pub fn emit_grow_owned_string(emitter: &mut impl Emitter, x86_scratch: &str) {
    abi::emit_call_label(emitter, "__rt_concat_grow");
    crate::heap_alloc::emit_stamp_result_kind(emitter, HeapKind::new(STRING_HEAP_KIND), x86_scratch);
}

/// Emits the direct owned-string allocation helper for the active target.
pub fn emit_str_new_owned(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_str_new_owned_x86_64(emitter);
        return;
    }
    emit_str_new_owned_aarch64(emitter);
}

/// Emits `__rt_str_new_owned` for AArch64.
fn emit_str_new_owned_aarch64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_new_owned ---");
    emitter.label_global("__rt_str_new_owned");

    abi::emit_symbol_address(emitter, "x9", "_heap_max");
    emitter.instruction("ldr x9, [x9]");                                        // load the configured heap capacity as the upper bound for one result
    emitter.instruction("cmp x0, x9");                                          // is the requested byte count impossible to satisfy (unsigned, so wrapped sizes are huge)?
    emitter.instruction("b.hi __rt_str_new_owned_too_large");                   // report a PHP-style allocation overflow instead of returning truncated storage
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the frame pointer and return address across the allocator call
    emitter.instruction("mov x29, sp");                                         // establish the allocation helper frame pointer
    emit_allocate_owned_string(emitter);
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the frame pointer and return address after the allocator call
    emitter.instruction("ret");                                                 // return the uninitialized owned payload to the caller

    emitter.label("__rt_str_new_owned_too_large");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits `__rt_str_new_owned` for Linux x86_64.
fn emit_str_new_owned_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_new_owned ---");
    emitter.label_global("__rt_str_new_owned");

    abi::emit_symbol_address(emitter, "r8", "_heap_max");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // load the configured heap capacity as the upper bound for one result
    emitter.instruction("cmp rax, r8");                                         // is the requested byte count impossible to satisfy (unsigned, so wrapped sizes are huge)?
    emitter.instruction("ja __rt_str_new_owned_too_large_x86");                 // report a PHP-style allocation overflow instead of returning truncated storage
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and realign the stack for the allocator call
    emitter.instruction("mov rbp, rsp");                                        // establish the allocation helper frame pointer
    emit_allocate_owned_string(emitter);
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the allocator call
    emitter.instruction("ret");                                                 // return the uninitialized owned payload to the caller

    emitter.label("__rt_str_new_owned_too_large_x86");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller
}
