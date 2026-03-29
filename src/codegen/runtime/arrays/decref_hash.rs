use crate::codegen::emit::Emitter;

pub fn emit_decref_hash(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_hash ---");
    emitter.label("__rt_decref_hash");

    // -- null check --
    emitter.instruction("cbz x0, __rt_decref_hash_skip");                       // skip if null pointer

    // -- heap range check: x0 >= _heap_buf --
    emitter.instruction("adrp x9, _heap_buf@PAGE");                             // load page of heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                       // resolve heap buffer base address
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_decref_hash_skip");                          // yes — not a heap pointer, skip

    // -- heap range check: x0 < _heap_buf + _heap_off --
    emitter.instruction("adrp x10, _heap_off@PAGE");                            // load page of heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                     // resolve heap offset address
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_decref_hash_skip");                          // yes — not a valid heap pointer, skip

    // -- debug mode: reject decref on freed storage --
    emitter.instruction("adrp x9, _heap_debug_enabled@PAGE");                   // load page of the heap-debug enabled flag
    emitter.instruction("add x9, x9, _heap_debug_enabled@PAGEOFF");             // resolve the heap-debug enabled flag address
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

    // -- non-zero refcount may indicate a now-unrooted cycle; run the targeted collector unless it is already active --
    emitter.instruction("adrp x9, _gc_release_suppressed@PAGE");                // load page of the release-suppression flag
    emitter.instruction("add x9, x9, _gc_release_suppressed@PAGEOFF");          // resolve the release-suppression flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the release-suppression flag
    emitter.instruction("cbnz x9, __rt_decref_hash_skip");                      // ordinary deep-free walks suppress nested collector runs
    emitter.instruction("adrp x9, _gc_collecting@PAGE");                        // load page of the collector-active flag
    emitter.instruction("add x9, x9, _gc_collecting@PAGEOFF");                  // resolve the collector-active flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the collector-active flag
    emitter.instruction("cbnz x9, __rt_decref_hash_skip");                      // nested decref calls during collection must not restart the collector
    emitter.instruction("ldr x9, [x0, #16]");                                   // load the runtime hash value_type tag from the container header
    emitter.instruction("cmp x9, #4");                                          // is this a hash of indexed arrays?
    emitter.instruction("b.eq __rt_decref_hash_collect");                       // nested heap payloads can participate in cycles
    emitter.instruction("cmp x9, #5");                                          // is this a hash of associative arrays?
    emitter.instruction("b.eq __rt_decref_hash_collect");                       // nested heap payloads can participate in cycles
    emitter.instruction("cmp x9, #6");                                          // is this a hash of objects?
    emitter.instruction("b.ne __rt_decref_hash_skip");                          // scalar/string hashes cannot participate in heap cycles
    emitter.label("__rt_decref_hash_collect");
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address across the collector call
    emitter.instruction("bl __rt_gc_collect_cycles");                           // reclaim any newly-unrooted refcounted graph components
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after collection
    emitter.instruction("b __rt_decref_hash_skip");                             // return after the optional collection pass

    // -- refcount reached zero: deep free the hash table and its owned entries --
    emitter.label("__rt_decref_hash_free");
    emitter.instruction("b __rt_hash_free_deep");                               // tail-call to deep free hash keys and heap-backed values

    emitter.label("__rt_decref_hash_skip");
    emitter.instruction("ret");                                                 // return to caller
}
