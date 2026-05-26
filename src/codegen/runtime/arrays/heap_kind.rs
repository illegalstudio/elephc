//! Purpose:
//! Emits the `__rt_heap_kind`, `__rt_heap_kind_zero` runtime helper assembly for heap kind.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Heap helpers own allocator metadata, debug accounting, and free-list invariants used by all refcounted runtime values.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the `__rt_heap_kind` runtime helper.
///
/// Returns the uniform heap kind tag for a heap-backed value by probing the
/// in-header kind word and validating the heap ownership marker.
///
/// ## Inputs
/// - `x0` / `rax`: heap user pointer (pointer into the managed heap)
///
/// ## Outputs
/// - `x0` / `eax`: kind tag (0 = not heap-backed, 1..255 = PHP array/hash kind)
///
/// ## Behavior
/// - Null pointers → returns 0 immediately.
/// - Pointers below the managed heap base → returns 0 (covers scalar integers and static data).
/// - Pointers at or beyond the heap extent → returns 0.
/// - On x86_64: validates the high-word ownership marker (`0x454c5048`) before returning the kind.
///   Foreign or freed pointers report kind 0.
/// - On ARM64: validates the heap range via `_heap_buf` / `_heap_off` symbols, loads kind word,
///   and masks to low byte.
///
/// ## ABI constraints
/// - Caller-saved registers used for transient heap address calculations.
/// - Result returned in the standard integer register (`x0` / `eax`).
pub fn emit_heap_kind(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: heap_kind ---");
        emitter.label_global("__rt_heap_kind");

        emitter.instruction("test rax, rax");                                   // null pointers have no heap kind
        emitter.instruction("jz __rt_heap_kind_zero");                          // return heap kind 0 for null pointers
        crate::codegen::abi::emit_symbol_address(emitter, "rcx", "_heap_buf");
        emitter.instruction("cmp rax, rcx");                                    // reject values below the managed x86_64 heap before probing metadata
        emitter.instruction("jb __rt_heap_kind_zero");                          // scalar integers and static data below the heap report kind 0
        crate::codegen::abi::emit_symbol_address(emitter, "rdx", "_heap_off");
        emitter.instruction("mov rdx, QWORD PTR [rdx]");                        // load the current x86_64 heap bump extent
        emitter.instruction("add rdx, rcx");                                    // compute the managed heap end address
        emitter.instruction("cmp rax, rdx");                                    // is the candidate pointer outside the live heap window?
        emitter.instruction("jae __rt_heap_kind_zero");                         // non-heap values above the managed heap report kind 0
        emitter.instruction("mov rcx, QWORD PTR [rax - 8]");                    // load the stamped x86_64 heap kind word from the uniform header
        emitter.instruction("mov rdx, rcx");                                    // preserve the low-byte heap kind before validating the ownership marker
        emitter.instruction("shr rcx, 32");                                     // isolate the high-word heap marker from the packed heap metadata
        emitter.instruction("cmp ecx, 0x454c5048");                             // verify that this pointer belongs to the elephc x86_64 heap runtime
        emitter.instruction("jne __rt_heap_kind_zero");                         // foreign or freed pointers report heap kind 0
        emitter.instruction("and edx, 0xff");                                   // isolate the low-byte uniform heap kind tag from the packed kind word
        emitter.instruction("mov eax, edx");                                    // return the low-byte heap kind tag in the integer result register
        emitter.instruction("ret");                                             // return the heap kind to the caller

        emitter.label("__rt_heap_kind_zero");
        emitter.instruction("xor eax, eax");                                    // report raw/non-heap kind 0
        emitter.instruction("ret");                                             // return default kind 0
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: heap_kind ---");
    emitter.label_global("__rt_heap_kind");

    // -- reject null pointers up front --
    emitter.instruction("cbz x0, __rt_heap_kind_zero");                         // null pointers have no heap kind

    // -- heap range check: x0 >= _heap_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the pointer below the heap base?
    emitter.instruction("b.lo __rt_heap_kind_zero");                            // non-heap pointers report kind 0

    // -- heap range check: x0 < _heap_buf + _heap_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current bump offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the pointer at or beyond the heap end?
    emitter.instruction("b.hs __rt_heap_kind_zero");                            // non-heap pointers report kind 0

    // -- load the uniform kind tag from the heap header --
    emitter.instruction("ldr x0, [x0, #-8]");                                   // load the full 64-bit heap kind word from the uniform header
    emitter.instruction("and x0, x0, #0xff");                                   // mask away packed value_type bits and transient GC metadata
    emitter.instruction("ret");                                                 // return the heap kind to the caller

    emitter.label("__rt_heap_kind_zero");
    emitter.instruction("mov x0, #0");                                          // report raw/non-heap kind 0
    emitter.instruction("ret");                                                 // return default kind 0
}
