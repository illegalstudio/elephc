//! Purpose:
//! Emits `__rt_str_new_owned`, the direct owned-heap string allocation used by string
//! transforms that can write their result exactly once.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Input: `x0` (AArch64) / `rax` (x86_64) = payload byte count.
//! - Output: `x0` / `rax` = payload pointer of a heap block stamped as heap kind 1
//!   (owned elephc string), ready to be filled by the caller.
//! - Requests above `_heap_max` take the same PHP allocation-overflow fatal as
//!   `__rt_concat_reserve`; the allocator rounds the payload to eight bytes.
//! - Callers must save their live state first: the allocator clobbers caller-saved
//!   registers on both targets.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, sentinels};

/// Emits the direct owned-string allocation helper for the active target.
pub fn emit_str_new_owned(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_new_owned_x86_64(emitter);
        return;
    }
    emit_str_new_owned_aarch64(emitter);
}

/// Emits `__rt_str_new_owned` for AArch64.
fn emit_str_new_owned_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_new_owned ---");
    emitter.label_global("__rt_str_new_owned");

    abi::emit_symbol_address(emitter, "x9", "_heap_max");
    emitter.instruction("ldr x9, [x9]");                                        // load the configured heap capacity as the upper bound for one result
    emitter.instruction("cmp x0, x9");                                          // is the requested byte count impossible to satisfy (unsigned, so wrapped sizes are huge)?
    emitter.instruction("b.hi __rt_str_new_owned_too_large");                   // report a PHP-style allocation overflow instead of returning truncated storage
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the frame pointer and return address across the allocator call
    emitter.instruction("mov x29, sp");                                         // establish the allocation helper frame pointer
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the exact owned payload on the managed heap
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the fresh allocation as an owned string before returning it
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the frame pointer and return address after the allocator call
    emitter.instruction("ret");                                                 // return the uninitialized owned payload to the caller

    emitter.label("__rt_str_new_owned_too_large");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits `__rt_str_new_owned` for Linux x86_64.
fn emit_str_new_owned_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_new_owned ---");
    emitter.label_global("__rt_str_new_owned");

    abi::emit_symbol_address(emitter, "r8", "_heap_max");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // load the configured heap capacity as the upper bound for one result
    emitter.instruction("cmp rax, r8");                                         // is the requested byte count impossible to satisfy (unsigned, so wrapped sizes are huge)?
    emitter.instruction("ja __rt_str_new_owned_too_large_x86");                 // report a PHP-style allocation overflow instead of returning truncated storage
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and realign the stack for the allocator call
    emitter.instruction("mov rbp, rsp");                                        // establish the allocation helper frame pointer
    emitter.instruction("call __rt_heap_alloc");                                // allocate the exact owned payload on the managed heap
    emitter.instruction(&format!("mov r10, 0x{:x}", sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string kind word with the heap magic marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the fresh allocation as an owned string before returning it
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the allocator call
    emitter.instruction("ret");                                                 // return the uninitialized owned payload to the caller

    emitter.label("__rt_str_new_owned_too_large_x86");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller
}
