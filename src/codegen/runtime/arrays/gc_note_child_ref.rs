use crate::codegen::emit::Emitter;

/// gc_note_child_ref: record one heap-to-heap incoming reference for a live refcounted child.
/// Input: x0 = child pointer
/// Output: none
pub fn emit_gc_note_child_ref(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gc_note_child_ref ---");
    emitter.label("__rt_gc_note_child_ref");

    // -- null and heap-range checks --
    emitter.instruction("cbz x0, __rt_gc_note_child_ref_done");                 // ignore null child pointers
    emitter.instruction("adrp x9, _heap_buf@PAGE");                             // load page of the heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                       // resolve the heap buffer base
    emitter.instruction("cmp x0, x9");                                          // is the child below the heap buffer?
    emitter.instruction("b.lo __rt_gc_note_child_ref_done");                    // only heap pointers participate in cycle accounting
    emitter.instruction("adrp x10, _heap_off@PAGE");                            // load page of the heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                     // resolve the heap offset address
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
