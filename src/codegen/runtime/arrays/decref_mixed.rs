use crate::codegen::emit::Emitter;

pub fn emit_decref_mixed(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_mixed ---");
    emitter.label_global("__rt_decref_mixed");

    emitter.instruction("cbz x0, __rt_decref_mixed_skip");                      // skip null mixed pointers immediately
    emitter.adrp("x9", "_heap_buf");                             // load page of heap buffer
    emitter.add_lo12("x9", "x9", "_heap_buf");                       // resolve heap buffer base address
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_decref_mixed_skip");                         // non-heap pointers need no mixed decref
    emitter.adrp("x10", "_heap_off");                            // load page of heap offset
    emitter.add_lo12("x10", "x10", "_heap_off");                     // resolve heap offset address
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_decref_mixed_skip");                         // invalid heap pointers must be ignored here

    emitter.adrp("x9", "_heap_debug_enabled");                   // load page of the heap-debug enabled flag
    emitter.add_lo12("x9", "x9", "_heap_debug_enabled");             // resolve the heap-debug enabled flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_decref_mixed_checked");                   // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the mixed cell is still live
    emitter.instruction("ldr x30, [sp], #16");                                  // restore return address after validation
    emitter.label("__rt_decref_mixed_checked");

    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the mixed cell refcount from the uniform header
    emitter.instruction("subs w9, w9, #1");                                     // decrement the mixed cell refcount and set flags
    emitter.instruction("str w9, [x0, #-12]");                                  // store the decremented mixed cell refcount
    emitter.instruction("b.eq __rt_decref_mixed_free");                         // zero refcount means the boxed payload can be released now

    emitter.adrp("x9", "_gc_release_suppressed");                // load page of the release-suppression flag
    emitter.add_lo12("x9", "x9", "_gc_release_suppressed");          // resolve the release-suppression flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the release-suppression flag
    emitter.instruction("cbnz x9, __rt_decref_mixed_skip");                     // ordinary deep-free walks suppress nested collector runs
    emitter.adrp("x9", "_gc_collecting");                        // load page of the collector-active flag
    emitter.add_lo12("x9", "x9", "_gc_collecting");                  // resolve the collector-active flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the collector-active flag
    emitter.instruction("cbnz x9, __rt_decref_mixed_skip");                     // nested decref calls during collection must not restart the collector
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed runtime payload tag
    emitter.instruction("cmp x9, #4");                                          // does the mixed cell point to an indexed array?
    emitter.instruction("b.eq __rt_decref_mixed_collect");                      // refcounted boxed children can participate in cycles
    emitter.instruction("cmp x9, #5");                                          // does the mixed cell point to an associative array?
    emitter.instruction("b.eq __rt_decref_mixed_collect");                      // refcounted boxed children can participate in cycles
    emitter.instruction("cmp x9, #6");                                          // does the mixed cell point to an object?
    emitter.instruction("b.eq __rt_decref_mixed_collect");                      // refcounted boxed children can participate in cycles
    emitter.instruction("cmp x9, #7");                                          // does the mixed cell point to another mixed cell?
    emitter.instruction("b.ne __rt_decref_mixed_skip");                         // scalar/string children cannot participate in heap cycles
    emitter.label("__rt_decref_mixed_collect");
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve return address across the collector call
    emitter.instruction("bl __rt_gc_collect_cycles");                           // reclaim any newly-unrooted graph components
    emitter.instruction("ldr x30, [sp], #16");                                  // restore return address after the collector call
    emitter.instruction("b __rt_decref_mixed_skip");                            // return after the optional collection pass

    emitter.label("__rt_decref_mixed_free");
    emitter.instruction("b __rt_mixed_free_deep");                              // tail-call to deep free the mixed cell and its boxed child

    emitter.label("__rt_decref_mixed_skip");
    emitter.instruction("ret");                                                 // nothing to release
}
