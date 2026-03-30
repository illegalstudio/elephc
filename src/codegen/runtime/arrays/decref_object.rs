use crate::codegen::emit::Emitter;

pub fn emit_decref_object(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_object ---");
    emitter.label("__rt_decref_object");

    // -- null check --
    emitter.instruction("cbz x0, __rt_decref_object_skip");                     // skip if null pointer

    // -- heap range check: x0 >= _heap_buf --
    emitter.instruction("adrp x9, _heap_buf@PAGE");                             // load page of heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                       // resolve heap buffer base address
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_decref_object_skip");                        // yes — not a heap pointer, skip

    // -- heap range check: x0 < _heap_buf + _heap_off --
    emitter.instruction("adrp x10, _heap_off@PAGE");                            // load page of heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                     // resolve heap offset address
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_decref_object_skip");                        // yes — not a valid heap pointer, skip

    // -- debug mode: reject decref on freed storage --
    emitter.instruction("adrp x9, _heap_debug_enabled@PAGE");                   // load page of the heap-debug enabled flag
    emitter.instruction("add x9, x9, _heap_debug_enabled@PAGEOFF");             // resolve the heap-debug enabled flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_decref_object_checked");                  // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the object block still has a live refcount
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.label("__rt_decref_object_checked");

    // -- decrement refcount and check for zero --
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load 32-bit refcount from the uniform heap header
    emitter.instruction("subs w9, w9, #1");                                     // decrement refcount, set flags
    emitter.instruction("str w9, [x0, #-12]");                                  // store decremented refcount
    emitter.instruction("b.eq __rt_decref_object_free");                        // zero refcount means the object can be freed immediately

    // -- non-zero refcount may indicate a now-unrooted cycle; run the targeted collector unless it is already active --
    emitter.instruction("adrp x9, _gc_release_suppressed@PAGE");                // load page of the release-suppression flag
    emitter.instruction("add x9, x9, _gc_release_suppressed@PAGEOFF");          // resolve the release-suppression flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the release-suppression flag
    emitter.instruction("cbnz x9, __rt_decref_object_skip");                    // ordinary deep-free walks suppress nested collector runs
    emitter.instruction("adrp x9, _gc_collecting@PAGE");                        // load page of the collector-active flag
    emitter.instruction("add x9, x9, _gc_collecting@PAGEOFF");                  // resolve the collector-active flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the collector-active flag
    emitter.instruction("cbnz x9, __rt_decref_object_skip");                    // nested decref calls during collection must not restart the collector
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address across the collector call
    emitter.instruction("bl __rt_gc_collect_cycles");                           // reclaim any newly-unrooted refcounted graph components
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after collection
    emitter.instruction("b __rt_decref_object_skip");                           // return after the optional collection pass

    // -- refcount reached zero: deep free the object --
    emitter.label("__rt_decref_object_free");
    emitter.instruction("b __rt_object_free_deep");                             // tail-call to deep free object properties and storage

    emitter.label("__rt_decref_object_skip");
    emitter.instruction("ret");                                                 // return to caller
}
