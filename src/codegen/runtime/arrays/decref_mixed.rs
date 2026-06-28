//! Purpose:
//! Emits the `__rt_decref_mixed`, `__rt_decref_mixed_skip` runtime helper assembly for decref mixed.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Decrement helpers are release paths for refcounted values; cycle collection
//!   runs only from explicit safe points after PHP-visible roots are updated.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits the `__rt_decref_mixed` runtime helper for the current target.
///
/// # Arguments
/// * `emitter` - The assembly emitter to write instructions into.
///
/// # Behavior
/// - On ARM64: validates the pointer lies within the managed heap, decrements the refcount
///   from the mixed cell header, and calls `__rt_mixed_free_deep` to release the payload
///   when the refcount reaches zero.
/// - On x86_64: validates the pointer is in-bounds with the correct heap magic header,
///   decrements the refcount, and tail-calls `__rt_mixed_free_deep` on zero refcount.
///
/// # ABI
/// - ARM64: input pointer in `x0`; clobbers `x9`, `x10`; preserves `x30` (link register).
/// - x86_64: input pointer in `rax`; clobbers `r10`, `r11`.
pub fn emit_decref_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_decref_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: decref_mixed ---");
    emitter.label_global("__rt_decref_mixed");

    emitter.instruction("cbz x0, __rt_decref_mixed_skip");                      // skip null mixed pointers immediately
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_decref_mixed_skip");                         // non-heap pointers need no mixed decref
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_decref_mixed_skip");                         // invalid heap pointers must be ignored here

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_debug_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_decref_mixed_checked");                   // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the mixed cell is still live
    emitter.instruction("ldr x30, [sp], #16");                                  // restore return address after validation
    emitter.label("__rt_decref_mixed_checked");

    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the mixed cell refcount from the uniform header
    emitter.instruction("subs w9, w9, #1");                                     // decrement the mixed cell refcount and set flags
    emitter.instruction("str w9, [x0, #-12]");                                  // store the decremented mixed cell refcount
    emitter.instruction("b.eq __rt_decref_mixed_free");                         // zero refcount means the boxed payload can be released now

    emitter.instruction("b __rt_decref_mixed_skip");                            // non-zero refcount stays alive until an explicit GC safe point

    emitter.label("__rt_decref_mixed_free");
    emitter.instruction("b __rt_mixed_free_deep");                              // tail-call to deep free the mixed cell and its boxed child

    emitter.label("__rt_decref_mixed_skip");
    emitter.instruction("ret");                                                 // nothing to release
}

/// Emits the `__rt_decref_mixed` runtime helper for the x86_64 Linux ABI.
///
/// # Arguments
/// * `emitter` - The assembly emitter to write instructions into.
///
/// # Behavior
/// - Skips null pointers and values below the managed heap base.
/// - Validates the pointer is within the live heap window and carries the
///   x86_64 heap magic marker in the high 32 bits of the header word.
/// - Decrements the refcount and leaves cycle collection to explicit safe points.
/// - Tail-calls `__rt_mixed_free_deep` on zero refcount.
///
/// # ABI
/// - Input pointer in `rax`; clobbers `r10`, `r11`; returns via `ret`.
fn emit_decref_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_mixed ---");
    emitter.label_global("__rt_decref_mixed");

    emitter.instruction("test rax, rax");                                       // skip null mixed pointers immediately because they do not own heap storage
    emitter.instruction("jz __rt_decref_mixed_skip");                           // null mixed values need no release work
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("lea r10, [r10 + 16]");                                 // first valid user payload begins after the initial heap header
    emitter.instruction("cmp rax, r10");                                        // reject null sentinels, scalar values, and static pointers before reading a heap header
    emitter.instruction("jb __rt_decref_mixed_skip");                           // non-heap values below the managed heap do not own mixed-box storage
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the current x86_64 heap bump extent before deriving the live heap end
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("add r11, r10");                                        // compute the managed heap end address from the base and live offset
    emitter.instruction("cmp rax, r11");                                        // is the candidate mixed pointer outside the live heap window?
    emitter.instruction("jae __rt_decref_mixed_skip");                          // pointers above the live heap end are not refcounted mixed boxes
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("shr r10, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r10d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // ignore foreign pointers that do not carry the elephc x86_64 heap marker
    emitter.instruction("jne __rt_decref_mixed_skip");                          // only elephc-owned mixed boxes participate in x86_64 decref bookkeeping
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the 32-bit mixed-box refcount from the uniform heap header
    emitter.instruction("sub r10d, 1");                                         // decrement the mixed-box refcount for the releasing x86_64 owner
    emitter.instruction("mov DWORD PTR [rax - 12], r10d");                      // store the decremented mixed-box refcount back into the uniform heap header
    emitter.instruction("jz __rt_decref_mixed_free");                           // zero refcount means the boxed payload can be released now
    emitter.instruction("jmp __rt_decref_mixed_skip");                          // non-zero refcount stays alive until an explicit GC safe point
    emitter.label("__rt_decref_mixed_skip");
    emitter.instruction("ret");                                                 // nothing else needs to happen for non-zero refcounts or foreign pointers

    emitter.label("__rt_decref_mixed_free");
    emitter.instruction("jmp __rt_mixed_free_deep");                            // tail-call to deep free the mixed box once the last owner is gone
}
