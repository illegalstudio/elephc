//! Purpose:
//! Emits the `__rt_decref_hash`, `__rt_decref_hash_skip` runtime helper assembly for decref hash.
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

/// Emits the `__rt_decref_hash` runtime helper.
///
/// Decrements the refcount of a heap-allocated PHP hash table.
/// Input: hash pointer in `x0` (ARM64) or `rax` (x86_64).
/// Behavior:
/// - Skips null, non-heap, and sentinel pointers immediately.
/// - Decrements refcount; if zero, tail-calls `__rt_hash_free_deep` for deep release.
/// - If refcount remains non-zero, returns without invoking cycle collection.
/// - On x86_64, additionally validates the heap magic marker before any operation.
///
/// ABI: preserves all caller-saved registers except `x0`/`rax` used for the optional
/// collector return value when a collection pass runs.
pub fn emit_decref_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_decref_hash_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: decref_hash ---");
    emitter.label_global("__rt_decref_hash");

    // -- null check --
    emitter.instruction("cbz x0, __rt_decref_hash_skip");                       // skip if null pointer

    // -- heap range check: x0 >= _heap_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_decref_hash_skip");                          // yes — not a heap pointer, skip

    // -- heap range check: x0 < _heap_buf + _heap_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_decref_hash_skip");                          // yes — not a valid heap pointer, skip

    // -- debug mode: reject decref on freed storage --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_debug_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_decref_hash_checked");                    // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the hash block still has a live refcount
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.label("__rt_decref_hash_checked");

    // -- decrement refcount and check for zero --
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load 32-bit refcount from the uniform heap header
    emitter.instruction("subs w9, w9, #1");                                     // decrement refcount, set flags
    emitter.instruction("str w9, [x0, #-12]");                                  // store decremented refcount
    emitter.instruction("b.eq __rt_decref_hash_free");                          // zero refcount means the hash can be freed immediately

    emitter.instruction("b __rt_decref_hash_skip");                             // non-zero refcount stays alive until an explicit GC safe point

    // -- refcount reached zero: deep free the hash table and its owned entries --
    emitter.label("__rt_decref_hash_free");
    emitter.instruction("b __rt_hash_free_deep");                               // tail-call to deep free hash keys and heap-backed values

    emitter.label("__rt_decref_hash_skip");
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux-specific path of `__rt_decref_hash`.
///
/// Target-specific lowering for x86_64; called from `emit_decref_hash` when
/// `emitter.target.arch == Arch::X86_64`.
/// Input: hash pointer in `rax`.
/// Differences from ARM64:
/// - Uses the x86_64 heap magic marker in the high 32 bits of the header word to
///   distinguish foreign pointers from eligphc-owned hash tables.
/// - Does not use a frame pointer or callee-saved registers; preserves `rbx`, `r12`–`r15`.
/// - `__rt_hash_free_deep` is reached through a tail jump.
fn emit_decref_hash_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_hash ---");
    emitter.label_global("__rt_decref_hash");

    emitter.instruction("test rax, rax");                                       // skip null hash pointers immediately because they do not own heap storage
    emitter.instruction("jz __rt_decref_hash_skip");                            // null hash values need no release work
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("lea r10, [r10 + 16]");                                 // first valid user payload begins after the initial heap header
    emitter.instruction("cmp rax, r10");                                        // reject null sentinels, scalar values, and static pointers before reading a heap header
    emitter.instruction("jb __rt_decref_hash_skip");                            // non-heap values below the managed heap do not own hash-table storage
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the current x86_64 heap bump extent before deriving the live heap end
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("add r11, r10");                                        // compute the managed heap end address from the base and live offset
    emitter.instruction("cmp rax, r11");                                        // is the candidate hash pointer outside the live heap window?
    emitter.instruction("jae __rt_decref_hash_skip");                           // pointers above the live heap end are not refcounted hashes
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("shr r10, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r10d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // ignore foreign pointers that do not carry the elephc x86_64 heap marker
    emitter.instruction("jne __rt_decref_hash_skip");                           // only elephc-owned hash tables participate in x86_64 decref bookkeeping
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the 32-bit hash refcount from the uniform heap header
    emitter.instruction("sub r10d, 1");                                         // decrement the hash refcount for the releasing x86_64 owner
    emitter.instruction("mov DWORD PTR [rax - 12], r10d");                      // store the decremented hash refcount back into the uniform heap header
    emitter.instruction("jz __rt_decref_hash_free");                            // zero refcount means the hash and its owned entries can be released now
    emitter.instruction("jmp __rt_decref_hash_skip");                           // non-zero refcount stays alive until an explicit GC safe point
    emitter.label("__rt_decref_hash_skip");
    emitter.instruction("ret");                                                 // nothing else needs to happen for non-zero refcounts or foreign pointers

    emitter.label("__rt_decref_hash_free");
    emitter.instruction("jmp __rt_hash_free_deep");                             // tail-call to deep free the hash table once the last owner is gone
}
