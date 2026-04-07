use crate::codegen::emit::Emitter;

/// decref_any: release a mixed heap-backed value using the uniform heap kind tag.
/// Input: x0 = heap-backed value pointer
/// Output: none
pub fn emit_decref_any(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_any ---");
    emitter.label_global("__rt_decref_any");

    // -- null and heap-range checks --
    emitter.instruction("cbz x0, __rt_decref_any_done");                        // skip null values immediately
    emitter.adrp("x9", "_heap_buf");                             // load page of the heap buffer
    emitter.add_lo12("x9", "x9", "_heap_buf");                       // resolve the heap buffer base
    emitter.instruction("cmp x0, x9");                                          // is the pointer below the heap buffer?
    emitter.instruction("b.lo __rt_decref_any_done");                           // non-heap values need no release
    emitter.adrp("x10", "_heap_off");                            // load page of the heap offset
    emitter.add_lo12("x10", "x10", "_heap_off");                     // resolve the heap offset address
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the pointer at or beyond the heap end?
    emitter.instruction("b.hs __rt_decref_any_done");                           // skip invalid or non-heap pointers

    // -- inspect the full kind word so collector-only flags stay visible --
    emitter.instruction("ldr x11, [x0, #-8]");                                  // load the full 64-bit kind word from the heap header

    // -- during cycle collection, skip unreachable refcounted children because they will be freed directly --
    emitter.adrp("x12", "_gc_collecting");                       // load page of the collector-active flag
    emitter.add_lo12("x12", "x12", "_gc_collecting");                // resolve the collector-active flag address
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
