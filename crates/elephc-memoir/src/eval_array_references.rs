//! Purpose:
//! Emits the optional callback that retires eval reference metadata before a boxed cell is freed.
//!
//! Called from:
//! - `super::heap_free::emit_heap_free()` when the eval bridge is enabled.
//!
//! Key details:
//! - The callback receives a live payload address and preserves it for the allocator.
//! - Heap kind 5 identifies a Mixed cell even if its former array payload has been overwritten.

use crate::abi;
use crate::Emitter;
use crate::Arch;

/// Retires boxed-cell metadata inline in heap_free, preserving its pointer and return address.
pub fn emit_eval_array_reference_retirement(emitter: &mut impl Emitter) {
    // -- notify eval before the allocation can be recycled for an unrelated array --
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldrb w9, [x0, #-8]");                          // inspect the allocation kind, not the mutable boxed value tag
            emitter.instruction("cmp w9, #5");                                  // only Mixed allocations can carry eval array-reference metadata
            emitter.instruction("b.ne __rt_heap_free_eval_references_done");    // skip allocations that cannot own this metadata
            abi::emit_symbol_address(emitter, "x10", "_elephc_eval_array_reference_retire_fn");
            emitter.instruction("ldr x10, [x10]");                              // load the optional retirement callback installed by eval
            emitter.instruction("cbz x10, __rt_heap_free_eval_references_done"); // keep non-eval programs independent of the Rust bridge
            emitter.instruction("stp x0, x30, [sp, #-16]!");                    // preserve the allocation and heap_free caller across the C callback
            emitter.instruction("blr x10");                                     // invalidate observers before the allocator recycles this address
            emitter.instruction("ldp x0, x30, [sp], #16");                      // restore allocator input and the original return address
        }
        Arch::X86_64 => {
            emitter.instruction("cmp BYTE PTR [rax - 8], 5");                   // only Mixed allocations can carry eval array-reference metadata
            emitter.instruction("jne __rt_heap_free_eval_references_done");     // skip allocations that cannot own this metadata
            abi::emit_symbol_address(emitter, "r10", "_elephc_eval_array_reference_retire_fn");
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // load the optional retirement callback installed by eval
            emitter.instruction("test r10, r10");                               // check whether eval has registered the metadata observer
            emitter.instruction("jz __rt_heap_free_eval_references_done");      // keep non-eval programs independent of the Rust bridge
            emitter.instruction("push rax");                                    // preserve the dying payload and align the SysV call from entry parity
            emitter.instruction("mov rdi, rax");                                // pass the validated cell address as the first C argument
            emitter.instruction("call r10");                                    // invalidate observers before the allocator recycles this address
            emitter.instruction("pop rax");                                     // restore the allocator input after the Rust callback
        }
    }
    emitter.label("__rt_heap_free_eval_references_done");
}
