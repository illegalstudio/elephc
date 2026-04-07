use crate::codegen::emit::Emitter;

pub fn emit_incref(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: incref ---");
    emitter.label_global("__rt_incref");

    // -- null check --
    emitter.instruction("cbz x0, __rt_incref_skip");                            // skip if null pointer

    // -- heap range check: x0 >= _heap_buf --
    emitter.adrp("x9", "_heap_buf");                             // load page of heap buffer
    emitter.add_lo12("x9", "x9", "_heap_buf");                       // resolve heap buffer base address
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_incref_skip");                               // yes — not a heap pointer, skip

    // -- heap range check: x0 < _heap_buf + _heap_off --
    emitter.adrp("x10", "_heap_off");                            // load page of heap offset
    emitter.add_lo12("x10", "x10", "_heap_off");                     // resolve heap offset address
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_incref_skip");                               // yes — not a valid heap pointer, skip

    // -- debug mode: reject incref on freed storage --
    emitter.adrp("x9", "_heap_debug_enabled");                   // load page of the heap-debug enabled flag
    emitter.add_lo12("x9", "x9", "_heap_debug_enabled");             // resolve the heap-debug enabled flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_incref_checked");                         // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the referenced heap block still has a live refcount
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.label("__rt_incref_checked");

    // -- increment refcount --
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load 32-bit refcount from the uniform heap header
    emitter.instruction("add w9, w9, #1");                                      // increment refcount
    emitter.instruction("str w9, [x0, #-12]");                                  // store incremented refcount

    emitter.label("__rt_incref_skip");
    emitter.instruction("ret");                                                 // return to caller
}
