//! Purpose:
//! Emits the `__rt_decref_object`, `__rt_decref_object_skip` runtime helper assembly for decref object.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Decrement helpers are release paths for refcounted values; cycle collection
//!   runs only from explicit safe points after PHP-visible roots are updated.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;


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
    crate::codegen_support::runtime::ctx::emit_heap_base_address(emitter, "x9");
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_decref_object_skip");                        // yes — not a heap pointer, skip

    // -- heap range check: x0 < _heap_buf + _heap_off --
    crate::codegen_support::runtime::ctx::emit_heap_off_load(emitter, "x10"); // x10 = current heap offset (ctx-relative in ctx mode)
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_decref_object_skip");                        // yes — not a valid heap pointer, skip

    // -- debug mode: reject decref on freed storage --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_heap_debug_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_decref_object_checked");                  // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the object block still has a live refcount
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.label("__rt_decref_object_checked");

    // -- terminal Parallel cleanup resumes a destructor interrupted by exit() --
    emitter.instruction(&format!("ldr x10, [x28, #{}]", crate::codegen_support::runtime::ctx::CTX_PARALLEL_FATAL_ACTIVE_OFFSET)); // inspect whether this worker is unwinding a terminal fatal
    emitter.instruction("cmp x10, #2");                                         // phase 2 means the fatal trampoline is draining abandoned owners
    emitter.instruction("b.ne __rt_decref_object_regular");                     // ordinary execution preserves the destructor re-entrancy guard
    emitter.instruction("ldr w10, [x0, #-12]");                                 // inspect the object's refcount and destructor-in-progress bit
    emitter.instruction("tbnz w10, #31, __rt_decref_object_finish_destructing"); // finish a destructor the fatal longjmp interrupted without calling it twice
    emitter.label("__rt_decref_object_regular");
    // -- decrement refcount and check for zero --
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load 32-bit refcount from the uniform heap header
    emitter.instruction("subs w9, w9, #1");                                     // decrement refcount, set flags
    emitter.instruction("str w9, [x0, #-12]");                                  // store decremented refcount
    emitter.instruction("b.eq __rt_decref_object_free");                        // zero refcount means the object can be freed immediately

    emitter.instruction("b __rt_decref_object_skip");                           // non-zero refcount stays alive until an explicit GC safe point

    // -- refcount reached zero: deep free the object --
    emitter.label("__rt_decref_object_free");
    emitter.instruction("b __rt_object_free_deep");                             // tail-call to deep free object properties and storage

    emitter.label("__rt_decref_object_finish_destructing");
    emitter.instruction("b __rt_object_free_deep");                             // resume the already-guarded deep free without re-entering __destruct

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
    crate::codegen_support::runtime::ctx::emit_heap_base_address(emitter, "r10");
    emitter.instruction("lea r10, [r10 + 16]");                                 // first valid user payload begins after the initial heap header
    emitter.instruction("cmp rax, r10");                                        // reject null sentinels, scalar values, and static pointers before reading a heap header
    emitter.instruction("jb __rt_decref_object_skip");                          // non-heap values below the managed heap do not own object storage
    crate::codegen_support::runtime::ctx::emit_heap_off_load(emitter, "r11"); // r11 = current heap offset (ctx-relative in ctx mode)
    crate::codegen_support::runtime::ctx::emit_heap_base_address(emitter, "r10");
    emitter.instruction("add r11, r10");                                        // compute the managed heap end address from the base and live offset
    emitter.instruction("cmp rax, r11");                                        // is the candidate object pointer outside the live heap window?
    emitter.instruction("jae __rt_decref_object_skip");                         // pointers above the live heap end are not refcounted objects
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("mov r11, r10");                                        // preserve the full heap kind word before isolating the ownership marker and heap kind
    emitter.instruction("shr r11, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r11d, 0x{:x}", crate::codegen_support::sentinels::X86_64_HEAP_MAGIC_HI32)); // ignore foreign pointers that do not carry the elephc x86_64 heap marker
    emitter.instruction("jne __rt_decref_object_skip");                         // only elephc-owned objects participate in x86_64 decref bookkeeping
    emitter.instruction("and r10, 0xff");                                       // isolate the low-byte uniform heap kind tag for a final ownership sanity check
    emitter.instruction("cmp r10, 4");                                          // is this heap-backed payload a plain object instance?
    emitter.instruction("je __rt_decref_object_counted");                       // plain objects release through the shared refcount bookkeeping
    emitter.instruction("cmp r10, 6");                                          // is this heap-backed payload a throwable object instance (issue #448)?
    emitter.instruction("jne __rt_decref_object_skip");                         // other heap kinds must not be released through the object decref helper

    emitter.label("__rt_decref_object_counted");
    // -- terminal Parallel cleanup resumes a destructor interrupted by exit() --
    emitter.instruction(&format!("mov r11, QWORD PTR [r14 + {}]", crate::codegen_support::runtime::ctx::CTX_PARALLEL_FATAL_ACTIVE_OFFSET)); // inspect whether this worker is unwinding a terminal fatal
    emitter.instruction("cmp r11, 2");                                          // phase 2 means the fatal trampoline is draining abandoned owners
    emitter.instruction("jne __rt_decref_object_regular_x86");                  // ordinary execution preserves the destructor re-entrancy guard
    emitter.instruction("mov r11d, DWORD PTR [rax - 12]");                      // inspect the object's refcount and destructor-in-progress bit
    emitter.instruction("test r11d, 0x80000000");                               // did an earlier destructor frame set the high-bit guard?
    emitter.instruction("jnz __rt_decref_object_finish_destructing_x86");       // finish it without invoking the destructor again
    emitter.label("__rt_decref_object_regular_x86");
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the 32-bit object refcount from the uniform heap header
    emitter.instruction("sub r10d, 1");                                         // decrement the object refcount for the releasing x86_64 owner
    emitter.instruction("mov DWORD PTR [rax - 12], r10d");                      // store the decremented object refcount back into the uniform heap header
    emitter.instruction("jz __rt_decref_object_free");                          // zero refcount means the object properties and storage can be released now
    emitter.instruction("jmp __rt_decref_object_skip");                         // non-zero refcount stays alive until an explicit GC safe point

    emitter.label("__rt_decref_object_skip");
    emitter.instruction("ret");                                                 // nothing else needs to happen for non-zero refcounts or foreign pointers

    emitter.label("__rt_decref_object_free");
    emitter.instruction("jmp __rt_object_free_deep");                           // tail-call to deep free the object once the last owner is gone

    emitter.label("__rt_decref_object_finish_destructing_x86");
    emitter.instruction("jmp __rt_object_free_deep");                           // resume the already-guarded deep free without re-entering __destruct
}
