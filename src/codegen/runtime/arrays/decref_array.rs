//! Purpose:
//! Emits the `__rt_decref_array`, `__rt_decref_array_skip` runtime helper assembly for decref array.
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

/// High 32 bits of the x86_64 Linux heap wrapper magic word.
/// Stored in the uniform heap header's kind word; verified before mutating refcount state
/// to distinguish foreign/static pointers from heap-backed array payloads.
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits the `__rt_decref_array` runtime helper.
pub fn emit_decref_array(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_decref_array_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: decref_array ---");
    emitter.label_global("__rt_decref_array");

    // -- null check --
    emitter.instruction("cbz x0, __rt_decref_array_skip");                      // skip if null pointer

    // -- heap range check: x0 >= _heap_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_decref_array_skip");                         // yes — not a heap pointer, skip

    // -- heap range check: x0 < _heap_buf + _heap_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_decref_array_skip");                         // yes — not a valid heap pointer, skip

    // -- debug mode: reject decref on freed storage --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_debug_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_decref_array_checked");                   // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the array block still has a live refcount
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.label("__rt_decref_array_checked");

    // -- decrement refcount and check for zero --
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load 32-bit refcount from the uniform heap header
    emitter.instruction("subs w9, w9, #1");                                     // decrement refcount, set flags
    emitter.instruction("str w9, [x0, #-12]");                                  // store decremented refcount
    emitter.instruction("b.eq __rt_decref_array_free");                         // zero refcount means the array can be freed immediately

    emitter.instruction("b __rt_decref_array_skip");                            // non-zero refcount stays alive until an explicit GC safe point

    // -- refcount reached zero: deep free the array --
    emitter.label("__rt_decref_array_free");
    emitter.instruction("b __rt_array_free_deep");                              // tail-call to deep free array + elements

    emitter.label("__rt_decref_array_skip");
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of the `__rt_decref_array` runtime helper.
///
/// Uses the OS x86_64 ABI: array pointer in `rax`, preserves all caller-saved registers,
/// and calls `__rt_array_free_deep` when the array's refcount reaches zero.
fn emit_decref_array_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_array ---");
    emitter.label_global("__rt_decref_array");

    emitter.instruction("test rax, rax");                                       // skip null array pointers so non-values do not participate in refcount traffic
    emitter.instruction("jz __rt_decref_array_skip");                           // null array pointers need no heap refcount update
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("lea r10, [r10 + 16]");                                 // first valid user payload begins after the initial heap header
    emitter.instruction("cmp rax, r10");                                        // reject null sentinels, scalar values, and static pointers before reading a heap header
    emitter.instruction("jb __rt_decref_array_skip");                           // non-heap values below the managed heap do not own indexed-array storage
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the current x86_64 heap bump extent before deriving the live heap end
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("add r11, r10");                                        // compute the managed heap end address from the base and live offset
    emitter.instruction("cmp rax, r11");                                        // is the candidate array pointer outside the live heap window?
    emitter.instruction("jae __rt_decref_array_skip");                          // pointers above the live heap end are not refcounted arrays
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("shr r10, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r10d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // verify that the payload is owned by the x86_64 heap wrapper before mutating refcount state
    emitter.instruction("jne __rt_decref_array_skip");                          // skip foreign/static pointers that do not carry elephc heap headers
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the 32-bit refcount stored in the uniform heap header
    emitter.instruction("sub r10d, 1");                                         // decrement the refcount for the array owner that is going away
    emitter.instruction("mov DWORD PTR [rax - 12], r10d");                      // persist the decremented array refcount in the uniform heap header
    emitter.instruction("jz __rt_decref_array_free");                           // zero refcount means the indexed array can be deep-freed immediately
    emitter.instruction("jmp __rt_decref_array_skip");                          // non-zero refcount stays alive until an explicit GC safe point

    emitter.label("__rt_decref_array_free");
    emitter.instruction("jmp __rt_array_free_deep");                            // tail-call into the indexed-array deep-free helper so nested heap-backed elements are released too
    emitter.label("__rt_decref_array_skip");
    emitter.instruction("ret");                                                 // return to the caller after the optional x86_64 array refcount update
}
