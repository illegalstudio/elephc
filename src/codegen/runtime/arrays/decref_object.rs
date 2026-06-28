//! Purpose:
//! Emits the `__rt_decref_object`, `__rt_decref_object_skip` runtime helper assembly for decref object.
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

/// Emits the `__rt_decref_object` runtime helper for ARM64.
///
/// Takes an object pointer in `x0`. Performs null check, heap-range validation,
/// and optional heap-debug liveness check. Decrements the refcount field stored at
/// `[x0 - 12]` in the uniform heap header. On zero refcount, tail-calls
/// `__rt_object_free_deep`. On non-zero refcount, returns without invoking the
/// cycle collector; explicit `GcCollect` safe points own cycle reclamation.
///
/// ## ABI constraints
/// - Input: `x0` = object pointer
/// - Output: `x0` preserved (returned unchanged)
/// - Clobbers: `x9`, `x10`, `x30` (link register preserved across collector call)
pub fn emit_decref_object(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_decref_object_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: decref_object ---");
    emitter.label_global("__rt_decref_object");

    // -- null check --
    emitter.instruction("cbz x0, __rt_decref_object_skip");                     // skip if null pointer

    // -- heap range check: x0 >= _heap_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_decref_object_skip");                        // yes — not a heap pointer, skip

    // -- heap range check: x0 < _heap_buf + _heap_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_decref_object_skip");                        // yes — not a valid heap pointer, skip

    // -- debug mode: reject decref on freed storage --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_debug_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_decref_object_checked");                  // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the object block still has a live refcount
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.label("__rt_decref_object_checked");

    // -- decrement refcount and check for zero --
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load 32-bit refcount from the uniform heap header
    emitter.instruction("subs w9, w9, #1");                                     // decrement refcount, set flags
    emitter.instruction("str w9, [x0, #-12]");                                  // store decremented refcount
    emitter.instruction("b.eq __rt_decref_object_free");                        // zero refcount means the object can be freed immediately

    emitter.instruction("b __rt_decref_object_skip");                           // non-zero refcount stays alive until an explicit GC safe point

    // -- refcount reached zero: deep free the object --
    emitter.label("__rt_decref_object_free");
    emitter.instruction("b __rt_object_free_deep");                             // tail-call to deep free object properties and storage

    emitter.label("__rt_decref_object_skip");
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the `__rt_decref_object` runtime helper for x86_64 Linux.
///
/// Takes an object pointer in `rax`. Performs null check, heap-range validation
/// using the x86_64 heap magic header word at `[rax - 8]`, and refcount decrement
/// at `[rax - 12]`. On zero refcount, tail-jumps to `__rt_object_free_deep`. On
/// non-zero refcount, returns without invoking the cycle collector.
///
/// ## ABI constraints
/// - Input: `rax` = object pointer
/// - Output: `rax` preserved
/// - Clobbers: `r10`, `r11`, caller-saved registers
fn emit_decref_object_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_object ---");
    emitter.label_global("__rt_decref_object");

    emitter.instruction("test rax, rax");                                       // skip null object pointers immediately because they do not own heap storage
    emitter.instruction("jz __rt_decref_object_skip");                          // null object values need no release work
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("lea r10, [r10 + 16]");                                 // first valid user payload begins after the initial heap header
    emitter.instruction("cmp rax, r10");                                        // reject null sentinels, scalar values, and static pointers before reading a heap header
    emitter.instruction("jb __rt_decref_object_skip");                          // non-heap values below the managed heap do not own object storage
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the current x86_64 heap bump extent before deriving the live heap end
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("add r11, r10");                                        // compute the managed heap end address from the base and live offset
    emitter.instruction("cmp rax, r11");                                        // is the candidate object pointer outside the live heap window?
    emitter.instruction("jae __rt_decref_object_skip");                         // pointers above the live heap end are not refcounted objects
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("mov r11, r10");                                        // preserve the full heap kind word before isolating the ownership marker and heap kind
    emitter.instruction("shr r11, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r11d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // ignore foreign pointers that do not carry the elephc x86_64 heap marker
    emitter.instruction("jne __rt_decref_object_skip");                         // only elephc-owned objects participate in x86_64 decref bookkeeping
    emitter.instruction("and r10, 0xff");                                       // isolate the low-byte uniform heap kind tag for a final ownership sanity check
    emitter.instruction("cmp r10, 4");                                          // is this heap-backed payload really an object instance?
    emitter.instruction("jne __rt_decref_object_skip");                         // other heap kinds must not be released through the object decref helper
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the 32-bit object refcount from the uniform heap header
    emitter.instruction("sub r10d, 1");                                         // decrement the object refcount for the releasing x86_64 owner
    emitter.instruction("mov DWORD PTR [rax - 12], r10d");                      // store the decremented object refcount back into the uniform heap header
    emitter.instruction("jz __rt_decref_object_free");                          // zero refcount means the object properties and storage can be released now
    emitter.instruction("jmp __rt_decref_object_skip");                         // non-zero refcount stays alive until an explicit GC safe point

    emitter.label("__rt_decref_object_skip");
    emitter.instruction("ret");                                                 // nothing else needs to happen for non-zero refcounts or foreign pointers

    emitter.label("__rt_decref_object_free");
    emitter.instruction("jmp __rt_object_free_deep");                           // tail-call to deep free the object once the last owner is gone
}
