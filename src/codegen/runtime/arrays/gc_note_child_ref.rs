//! Purpose:
//! Emits the `__rt_gc_note_child_ref`, `__rt_gc_note_child_ref_done` runtime helper assembly for gc note child ref.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - GC helpers must honor cycle-collection suppression, mark bits, and parent/child references without double-releasing values.

use crate::codegen::emit::Emitter;

/// Emits `__rt_gc_note_child_ref`, which records one heap-to-heap incoming edge for
/// cycle-aware GC. Skips null pointers, out-of-range pointers, freed blocks, and
/// non-refcounted kinds (strings/raw buffers). For valid refcounted array/hash/object
/// children, bumps the transient incoming-edge counter stored in the high 32 bits of
/// the child block's kind word.
///
/// # Inputs
/// - `x0`: child block pointer (a heap-allocated refcounted array/hash/object)
///
/// # Outputs
/// - None (registers `x0`–`x14` are clobbered)
///
/// # ABI
/// - AAPCS64: caller-saved registers `x0`–`x17` may be clobbered
pub fn emit_gc_note_child_ref(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gc_note_child_ref ---");
    emitter.label_global("__rt_gc_note_child_ref");

    // -- null and heap-range checks --
    emitter.instruction("cbz x0, __rt_gc_note_child_ref_done");                 // ignore null child pointers
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the child below the heap buffer?
    emitter.instruction("b.lo __rt_gc_note_child_ref_done");                    // only heap pointers participate in cycle accounting
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the child at or beyond the current heap end?
    emitter.instruction("b.hs __rt_gc_note_child_ref_done");                    // invalid pointers contribute nothing

    // -- only live refcounted array/hash/object blocks contribute to incoming heap-edge counts --
    emitter.instruction("ldr w11, [x0, #-12]");                                 // load the child refcount from the heap header
    emitter.instruction("cbz w11, __rt_gc_note_child_ref_done");                // freed blocks are not part of the graph
    emitter.instruction("ldr x12, [x0, #-8]");                                  // load the full child kind word
    emitter.instruction("and x13, x12, #0xff");                                 // isolate the low-byte heap kind tag
    emitter.instruction("cmp x13, #2");                                         // is this at least an indexed array?
    emitter.instruction("b.lo __rt_gc_note_child_ref_done");                    // strings/raw buffers do not participate in cycle accounting
    emitter.instruction("cmp x13, #4");                                         // is this within the array/hash/object range?
    emitter.instruction("b.hi __rt_gc_note_child_ref_done");                    // ignore unknown/raw heap kinds

    // -- bump the transient incoming-edge counter stored in the high 32 bits of the kind word --
    emitter.instruction("mov x14, #1");                                         // prepare a single incoming-edge increment
    emitter.instruction("lsl x14, x14, #32");                                   // move the increment into the high 32 bits
    emitter.instruction("add x12, x12, x14");                                   // add one heap-incoming edge to this child block
    emitter.instruction("str x12, [x0, #-8]");                                  // persist the updated incoming-edge count

    emitter.label("__rt_gc_note_child_ref_done");
    emitter.instruction("ret");                                                 // return to the caller
}
