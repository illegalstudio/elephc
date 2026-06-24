//! Purpose:
//! Emits the `__rt_decref_refcell` runtime helper assembly for releasing one reference-cell owner.
//! Decrements a heap-kind-6 reference cell's refcount and deep-frees it when the last owner leaves.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//! - `__rt_decref_any` (kind 6), hash/array/mixed deep-free paths, and source-variable teardown.
//!
//! Key details:
//! - The refcount discipline matches `__rt_decref_mixed`: one decref per distinct owner, deep-free
//!   at zero. When the inner value is a heap-backed child (tags 4-7) a targeted cycle collection
//!   runs so reference cells can participate in graph cleanup like boxed mixed cells.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// High 32 bits of the x86_64 heap-block kind word, used to reject foreign pointers. ASCII `"ELPH"`.
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits the `__rt_decref_refcell` runtime helper for the current target.
///
/// # Behavior
/// - Validates the pointer lies within the managed heap, decrements the refcount in the cell header,
///   and tail-calls `__rt_refcell_free_deep` when the refcount reaches zero.
/// - For surviving owners whose inner value is a heap-backed child (tags 4-7), triggers a targeted
///   cycle collection unless suppressed or already collecting.
///
/// # ABI
/// - ARM64: input pointer in `x0`; clobbers `x9`, `x10`; preserves `x30`.
/// - x86_64: input pointer in `rax`; clobbers `r10`, `r11`.
pub fn emit_decref_refcell(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_decref_refcell_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: decref_refcell ---");
    emitter.label_global("__rt_decref_refcell");

    emitter.instruction("cbz x0, __rt_decref_refcell_skip");                    // skip null reference pointers immediately
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the pointer below the heap start?
    emitter.instruction("b.lo __rt_decref_refcell_skip");                       // non-heap pointers need no reference decref
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the pointer at or beyond the heap end?
    emitter.instruction("b.hs __rt_decref_refcell_skip");                       // invalid heap pointers must be ignored here

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_debug_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_decref_refcell_checked");                 // skip debug validation when heap-debug is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the reference cell is still live
    emitter.instruction("ldr x30, [sp], #16");                                  // restore return address after validation
    emitter.label("__rt_decref_refcell_checked");

    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the reference cell refcount from the uniform header
    emitter.instruction("subs w9, w9, #1");                                     // decrement the reference cell refcount and set flags
    emitter.instruction("str w9, [x0, #-12]");                                  // store the decremented reference cell refcount
    emitter.instruction("b.eq __rt_decref_refcell_free");                       // zero refcount means the cell can be released now

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_release_suppressed");
    emitter.instruction("ldr x9, [x9]");                                        // load the release-suppression flag
    emitter.instruction("cbnz x9, __rt_decref_refcell_skip");                   // ordinary deep-free walks suppress nested collector runs
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_collecting");
    emitter.instruction("ldr x9, [x9]");                                        // load the collector-active flag
    emitter.instruction("cbnz x9, __rt_decref_refcell_skip");                   // nested decref calls during collection must not restart the collector
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed inner value tag
    emitter.instruction("cmp x9, #4");                                          // does the cell reference an indexed array?
    emitter.instruction("b.eq __rt_decref_refcell_collect");                    // refcounted inner children can participate in cycles
    emitter.instruction("cmp x9, #5");                                          // does the cell reference an associative array?
    emitter.instruction("b.eq __rt_decref_refcell_collect");                    // refcounted inner children can participate in cycles
    emitter.instruction("cmp x9, #6");                                          // does the cell reference an object?
    emitter.instruction("b.eq __rt_decref_refcell_collect");                    // refcounted inner children can participate in cycles
    emitter.instruction("cmp x9, #7");                                          // does the cell reference a boxed mixed cell?
    emitter.instruction("b.ne __rt_decref_refcell_skip");                       // scalar/string inner values cannot participate in heap cycles
    emitter.label("__rt_decref_refcell_collect");
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve return address across the collector call
    emitter.instruction("bl __rt_gc_collect_cycles");                           // reclaim any newly-unrooted graph components
    emitter.instruction("ldr x30, [sp], #16");                                  // restore return address after the collector call
    emitter.instruction("b __rt_decref_refcell_skip");                          // return after the optional collection pass

    emitter.label("__rt_decref_refcell_free");
    emitter.instruction("b __rt_refcell_free_deep");                            // tail-call to deep free the cell and its inner child

    emitter.label("__rt_decref_refcell_skip");
    emitter.instruction("ret");                                                 // nothing to release
}

/// Emits the `__rt_decref_refcell` runtime helper for the x86_64 Linux ABI.
///
/// Mirrors `__rt_decref_mixed`: validates the heap magic marker, decrements the refcount, runs a
/// targeted cycle collection for heap-backed inner children (tags 4-7), and tail-calls
/// `__rt_refcell_free_deep` on zero refcount.
///
/// # ABI
/// - Input pointer in `rax`; clobbers `r10`, `r11`; returns via `ret`.
fn emit_decref_refcell_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_refcell ---");
    emitter.label_global("__rt_decref_refcell");

    emitter.instruction("test rax, rax");                                       // skip null reference pointers immediately because they own no storage
    emitter.instruction("jz __rt_decref_refcell_skip");                         // null reference values need no release work
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("lea r10, [r10 + 16]");                                 // first valid user payload begins after the initial heap header
    emitter.instruction("cmp rax, r10");                                        // reject null sentinels, scalars, and static pointers before reading a header
    emitter.instruction("jb __rt_decref_refcell_skip");                         // non-heap values below the managed heap own no reference storage
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the current heap bump extent before deriving the live heap end
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("add r11, r10");                                        // compute the managed heap end address from the base and live offset
    emitter.instruction("cmp rax, r11");                                        // is the candidate reference pointer outside the live heap window?
    emitter.instruction("jae __rt_decref_refcell_skip");                        // pointers above the live heap end are not refcounted reference cells
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("shr r10, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r10d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // ignore foreign pointers that lack the elephc x86_64 heap marker
    emitter.instruction("jne __rt_decref_refcell_skip");                        // only elephc-owned reference cells participate in x86_64 decref bookkeeping
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the 32-bit reference cell refcount from the uniform header
    emitter.instruction("sub r10d, 1");                                         // decrement the reference cell refcount for the releasing owner
    emitter.instruction("mov DWORD PTR [rax - 12], r10d");                      // store the decremented reference cell refcount back into the header
    emitter.instruction("jz __rt_decref_refcell_free");                         // zero refcount means the cell can be released now
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_gc_release_suppressed");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the release-suppression flag before a targeted cycle-collector run
    emitter.instruction("test r11, r11");                                       // is this decref happening inside an ordinary deep-free walk?
    emitter.instruction("jnz __rt_decref_refcell_skip");                        // yes — nested collector runs stay suppressed during deep frees
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_gc_collecting");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the collector-active flag before attempting another collection pass
    emitter.instruction("test r11, r11");                                       // is the collector already running?
    emitter.instruction("jnz __rt_decref_refcell_skip");                        // yes — nested decref calls during collection must not restart the collector
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // load the boxed inner value tag before deciding whether it can cycle
    emitter.instruction("cmp r11, 4");                                          // does this cell currently reference a heap-backed child?
    emitter.instruction("jb __rt_decref_refcell_skip");                         // scalar, string, and null inner values cannot participate in heap cycles
    emitter.instruction("cmp r11, 7");                                          // is the inner runtime tag within the supported heap-backed range?
    emitter.instruction("ja __rt_decref_refcell_skip");                         // unknown inner runtime tags are ignored by the x86_64 collector trigger
    emitter.instruction("call __rt_gc_collect_cycles");                         // reclaim any newly unrooted graph components reachable through the cell
    emitter.instruction("jmp __rt_decref_refcell_skip");                        // return after the optional x86_64 collector pass
    emitter.label("__rt_decref_refcell_skip");
    emitter.instruction("ret");                                                 // nothing else needs to happen for non-zero refcounts or foreign pointers

    emitter.label("__rt_decref_refcell_free");
    emitter.instruction("jmp __rt_refcell_free_deep");                          // tail-call to deep free the cell once the last owner is gone
}
