use crate::codegen::emit::Emitter;

/// heap_kind: return the uniform heap kind tag for a heap-backed value.
/// Input: x0 = heap user pointer
/// Output: x0 = kind tag (0 for null/non-heap/raw allocations)
pub fn emit_heap_kind(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_kind ---");
    emitter.label("__rt_heap_kind");

    // -- reject null pointers up front --
    emitter.instruction("cbz x0, __rt_heap_kind_zero");                          // null pointers have no heap kind

    // -- heap range check: x0 >= _heap_buf --
    emitter.instruction("adrp x9, _heap_buf@PAGE");                              // load page of the heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                        // resolve the heap buffer base address
    emitter.instruction("cmp x0, x9");                                           // is the pointer below the heap base?
    emitter.instruction("b.lo __rt_heap_kind_zero");                             // non-heap pointers report kind 0

    // -- heap range check: x0 < _heap_buf + _heap_off --
    emitter.instruction("adrp x10, _heap_off@PAGE");                             // load page of the current heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                      // resolve the heap offset address
    emitter.instruction("ldr x10, [x10]");                                       // load the current bump offset
    emitter.instruction("add x10, x9, x10");                                     // compute the current heap end
    emitter.instruction("cmp x0, x10");                                          // is the pointer at or beyond the heap end?
    emitter.instruction("b.hs __rt_heap_kind_zero");                             // non-heap pointers report kind 0

    // -- load the uniform kind tag from the heap header --
    emitter.instruction("ldr x0, [x0, #-8]");                                    // load the 64-bit heap kind tag from the uniform header
    emitter.instruction("ret");                                                  // return the heap kind to the caller

    emitter.label("__rt_heap_kind_zero");
    emitter.instruction("mov x0, #0");                                           // report raw/non-heap kind 0
    emitter.instruction("ret");                                                  // return default kind 0
}
