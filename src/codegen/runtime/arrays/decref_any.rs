//! Purpose:
//! Emits the `__rt_decref_any`, `__rt_decref_any_done` runtime helper assembly for decref any.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Decrement helpers are release paths for refcounted values and must balance recursive frees with GC cycle collection.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Magic high-32 bits marker injected into x86_64 heap wrapper headers to distinguish
/// managed heap payloads from foreign/static pointers during release validation.
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Uniform release dispatcher for mixed heap-backed values.
///
/// Reads the heap kind tag from the value's header, validates the pointer against the
/// managed heap window, and dispatches to the appropriate concrete release helper
/// (string, array, hash, object, mixed). Skips GC-tracked children during active cycle
/// collection to avoid double-frees when the collector reclaims them directly.
///
/// Input: x0 (ARM64) or rax (x86_64) = heap-backed value pointer
/// Output: none
pub fn emit_decref_any(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_decref_any_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: decref_any ---");
    emitter.label_global("__rt_decref_any");

    // -- null and heap-range checks --
    emitter.instruction("cbz x0, __rt_decref_any_done");                        // skip null values immediately
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the pointer below the heap buffer?
    emitter.instruction("b.lo __rt_decref_any_done");                           // non-heap values need no release
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the pointer at or beyond the heap end?
    emitter.instruction("b.hs __rt_decref_any_done");                           // skip invalid or non-heap pointers

    // -- inspect the full kind word so collector-only flags stay visible --
    emitter.instruction("ldr x11, [x0, #-8]");                                  // load the full 64-bit kind word from the heap header

    // -- during cycle collection, skip unreachable refcounted children because they will be freed directly --
    crate::codegen::abi::emit_symbol_address(emitter, "x12", "_gc_collecting");
    emitter.instruction("ldr x12, [x12]");                                      // load the collector-active flag
    emitter.instruction("cbz x12, __rt_decref_any_dispatch");                   // ordinary release path when no collection is running
    emitter.instruction("and x13, x11, #0xff");                                 // isolate the low-byte heap kind tag
    emitter.instruction("cmp x13, #2");                                         // is this a refcounted indexed array?
    emitter.instruction("b.lo __rt_decref_any_dispatch");                       // strings should still be freed immediately
    emitter.instruction("cmp x13, #5");                                         // is this within the refcounted array/hash/object/mixed range?
    emitter.instruction("b.hi __rt_decref_any_dispatch");                       // raw/untyped blocks are not part of refcounted graph cleanup
    emitter.instruction("mov x14, #1");                                         // prepare a single-bit reachable mask
    emitter.instruction("lsl x14, x14, #16");                                   // x14 = GC reachable bit in the kind word
    emitter.instruction("tst x11, x14");                                        // does this child stay reachable from an external root?
    emitter.instruction("b.eq __rt_decref_any_done");                           // unreachable refcounted children are reclaimed by the collector itself

    // -- dispatch to the concrete release routine --
    emitter.label("__rt_decref_any_dispatch");
    emitter.instruction("and x11, x11, #0xff");                                 // keep only the low-byte heap kind tag
    emitter.instruction("cmp x11, #1");                                         // is this an owned string buffer?
    emitter.instruction("b.eq __rt_decref_any_string");                         // release strings via heap_free_safe
    emitter.instruction("cmp x11, #2");                                         // is this an indexed array?
    emitter.instruction("b.eq __rt_decref_any_array");                          // release arrays through __rt_decref_array
    emitter.instruction("cmp x11, #3");                                         // is this an associative array / hash?
    emitter.instruction("b.eq __rt_decref_any_hash");                           // release hashes through __rt_decref_hash
    emitter.instruction("cmp x11, #4");                                         // is this an object instance?
    emitter.instruction("b.eq __rt_decref_any_object");                         // release objects through __rt_decref_object
    emitter.instruction("cmp x11, #5");                                         // is this a boxed mixed value?
    emitter.instruction("b.eq __rt_decref_any_mixed");                          // release mixed cells through __rt_decref_mixed
    emitter.instruction("ret");                                                 // unknown/raw kinds need no release

    emitter.label("__rt_decref_any_string");
    emitter.instruction("b __rt_heap_free_safe");                               // tail-call to owned string release

    emitter.label("__rt_decref_any_array");
    emitter.instruction("b __rt_decref_array");                                 // tail-call to array decref

    emitter.label("__rt_decref_any_hash");
    emitter.instruction("b __rt_decref_hash");                                  // tail-call to hash decref

    emitter.label("__rt_decref_any_object");
    emitter.instruction("b __rt_decref_object");                                // tail-call to object decref

    emitter.label("__rt_decref_any_mixed");
    emitter.instruction("b __rt_decref_mixed");                                 // tail-call to mixed-cell decref

    emitter.label("__rt_decref_any_done");
    emitter.instruction("ret");                                                 // nothing to release
}

/// x86_64 Linux implementation of the uniform release dispatcher.
/// Uses the x86_64 heap wrapper header format with a high-word magic marker to distinguish
/// managed heap payloads from foreign or static pointers before dispatching to concrete
/// release helpers (string, array, hash, object, mixed).
/// Input: rax = heap-backed value pointer
/// Output: none (tail-calls to specialized release helpers)
fn emit_decref_any_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_any ---");
    emitter.label_global("__rt_decref_any");

    emitter.instruction("test rax, rax");                                       // skip null heap-backed payload pointers so non-values do not participate in x86_64 release traffic
    emitter.instruction("jz __rt_decref_any_done");                             // null payloads own no heap storage and therefore need no release work
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("cmp rax, r10");                                        // reject values below the managed x86_64 heap before reading a header word
    emitter.instruction("jb __rt_decref_any_done");                             // scalar integers and static data below the heap own no runtime storage
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the current x86_64 heap bump extent
    emitter.instruction("add r11, r10");                                        // compute the managed heap end address
    emitter.instruction("cmp rax, r11");                                        // is the candidate pointer outside the live heap window?
    emitter.instruction("jae __rt_decref_any_done");                            // non-heap values above the managed heap own no runtime storage
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("mov r11, r10");                                        // preserve the full heap kind word before isolating the ownership marker
    emitter.instruction("shr r11, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r11d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // verify that the payload belongs to the x86_64 heap wrapper before dispatching a release helper
    emitter.instruction("jne __rt_decref_any_done");                            // foreign/static pointers must be ignored by the uniform x86_64 release dispatcher
    emitter.instruction("and r10, 0xff");                                       // isolate the low-byte uniform heap kind tag for the concrete release dispatch
    emitter.instruction("cmp r10, 1");                                          // does this heap-backed payload own a persisted string buffer?
    emitter.instruction("je __rt_decref_any_string");                           // strings release through heap_free_safe on x86_64
    emitter.instruction("cmp r10, 2");                                          // does this heap-backed payload point at an indexed array?
    emitter.instruction("je __rt_decref_any_array");                            // indexed arrays release through the x86_64 array decref helper
    emitter.instruction("cmp r10, 3");                                          // does this heap-backed payload point at an associative array?
    emitter.instruction("je __rt_decref_any_hash");                             // hashes release through the x86_64 hash decref helper
    emitter.instruction("cmp r10, 4");                                          // does this heap-backed payload point at an object instance?
    emitter.instruction("je __rt_decref_any_object");                           // objects release through the x86_64 object decref helper
    emitter.instruction("cmp r10, 5");                                          // does this heap-backed payload point at a boxed mixed cell?
    emitter.instruction("je __rt_decref_any_mixed");                            // mixed cells release through the x86_64 mixed decref helper
    emitter.instruction("jmp __rt_decref_any_done");                            // unknown/raw heap kinds need no release work in the current x86_64 bootstrap runtime

    emitter.label("__rt_decref_any_string");
    emitter.instruction("jmp __rt_heap_free_safe");                             // tail-call to the persisted-string safe-free helper on x86_64

    emitter.label("__rt_decref_any_array");
    emitter.instruction("jmp __rt_decref_array");                               // tail-call to the indexed-array decref helper on x86_64

    emitter.label("__rt_decref_any_hash");
    emitter.instruction("jmp __rt_decref_hash");                                // tail-call to the associative-array decref helper on x86_64

    emitter.label("__rt_decref_any_object");
    emitter.instruction("jmp __rt_decref_object");                              // tail-call to the object decref helper on x86_64

    emitter.label("__rt_decref_any_mixed");
    emitter.instruction("jmp __rt_decref_mixed");                               // tail-call to the mixed-box decref helper on x86_64

    emitter.label("__rt_decref_any_done");
    emitter.instruction("ret");                                                 // nothing to release for null, foreign, or unsupported heap kinds
}
