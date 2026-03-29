use crate::codegen::emit::Emitter;

/// Reference counting runtime functions for the garbage collector.
/// Refcount is stored as a 32-bit value at [user_ptr - 4] (upper half of heap header).
pub fn emit_refcount(emitter: &mut Emitter) {
    // __rt_incref: increment reference count
    // Input: x0 = user pointer (as returned by heap_alloc)
    // Safe to call with null or non-heap pointers (silently skips)
    emit_incref(emitter);

    // __rt_decref_array: decrement refcount, deep-free indexed array if zero
    emit_decref_array(emitter);

    // __rt_decref_hash: decrement refcount, free hash table if zero
    emit_decref_hash(emitter);

    // __rt_decref_object: decrement refcount, free object if zero
    emit_decref_object(emitter);
}

fn emit_incref(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: incref ---");
    emitter.label("__rt_incref");

    // -- null check --
    emitter.instruction("cbz x0, __rt_incref_skip");                            // skip if null pointer

    // -- heap range check: x0 >= _heap_buf --
    emitter.instruction("adrp x9, _heap_buf@PAGE");                             // load page of heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                       // resolve heap buffer base address
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_incref_skip");                               // yes — not a heap pointer, skip

    // -- heap range check: x0 < _heap_buf + _heap_off --
    emitter.instruction("adrp x10, _heap_off@PAGE");                            // load page of heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                     // resolve heap offset address
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_incref_skip");                               // yes — not a valid heap pointer, skip

    // -- debug mode: reject incref on freed storage --
    emitter.instruction("adrp x9, _heap_debug_enabled@PAGE");                   // load page of the heap-debug enabled flag
    emitter.instruction("add x9, x9, _heap_debug_enabled@PAGEOFF");             // resolve the heap-debug enabled flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_incref_checked");                         // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the referenced heap block still has a live refcount
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.label("__rt_incref_checked");

    // -- increment refcount --
    emitter.instruction("ldr w9, [x0, #-4]");                                   // load 32-bit refcount from header
    emitter.instruction("add w9, w9, #1");                                      // increment refcount
    emitter.instruction("str w9, [x0, #-4]");                                   // store incremented refcount

    emitter.label("__rt_incref_skip");
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_decref_array(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_array ---");
    emitter.label("__rt_decref_array");

    // -- null check --
    emitter.instruction("cbz x0, __rt_decref_array_skip");                      // skip if null pointer

    // -- heap range check: x0 >= _heap_buf --
    emitter.instruction("adrp x9, _heap_buf@PAGE");                             // load page of heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                       // resolve heap buffer base address
    emitter.instruction("cmp x0, x9");                                          // is pointer below heap start?
    emitter.instruction("b.lo __rt_decref_array_skip");                         // yes — not a heap pointer, skip

    // -- heap range check: x0 < _heap_buf + _heap_off --
    emitter.instruction("adrp x10, _heap_off@PAGE");                            // load page of heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                     // resolve heap offset address
    emitter.instruction("ldr x10, [x10]");                                      // x10 = current heap offset
    emitter.instruction("add x10, x9, x10");                                    // x10 = heap_buf + heap_off = heap end
    emitter.instruction("cmp x0, x10");                                         // is pointer at or beyond heap end?
    emitter.instruction("b.hs __rt_decref_array_skip");                         // yes — not a valid heap pointer, skip

    // -- debug mode: reject decref on freed storage --
    emitter.instruction("adrp x9, _heap_debug_enabled@PAGE");                   // load page of the heap-debug enabled flag
    emitter.instruction("add x9, x9, _heap_debug_enabled@PAGEOFF");             // resolve the heap-debug enabled flag address
    emitter.instruction("ldr x9, [x9]");                                        // load the heap-debug enabled flag
    emitter.instruction("cbz x9, __rt_decref_array_checked");                   // skip debug validation when heap-debug mode is disabled
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the caller return address before nested validation
    emitter.instruction("bl __rt_heap_debug_check_live");                       // ensure the array block still has a live refcount
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the caller return address after validation
    emitter.label("__rt_decref_array_checked");

    // -- decrement refcount and check for zero --
    emitter.instruction("ldr w9, [x0, #-4]");                                   // load 32-bit refcount from header
    emitter.instruction("subs w9, w9, #1");                                     // decrement refcount, set flags
    emitter.instruction("str w9, [x0, #-4]");                                   // store decremented refcount
    emitter.instruction("b.ne __rt_decref_array_skip");                         // if not zero, still referenced — skip free

    // -- refcount reached zero: deep free the array --
    emitter.instruction("b __rt_array_free_deep");                              // tail-call to deep free array + elements

    emitter.label("__rt_decref_array_skip");
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_decref_hash(emitter: &mut Emitter) {
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
    emitter.instruction("ldr w9, [x0, #-4]");                                   // load 32-bit refcount from header
    emitter.instruction("subs w9, w9, #1");                                     // decrement refcount, set flags
    emitter.instruction("str w9, [x0, #-4]");                                   // store decremented refcount
    emitter.instruction("b.ne __rt_decref_hash_skip");                          // if not zero, still referenced — skip free

    // -- refcount reached zero: deep free the hash table and its owned entries --
    emitter.instruction("b __rt_hash_free_deep");                               // tail-call to deep free hash keys and heap-backed values

    emitter.label("__rt_decref_hash_skip");
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_decref_object(emitter: &mut Emitter) {
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
    emitter.instruction("ldr w9, [x0, #-4]");                                   // load 32-bit refcount from header
    emitter.instruction("subs w9, w9, #1");                                     // decrement refcount, set flags
    emitter.instruction("str w9, [x0, #-4]");                                   // store decremented refcount
    emitter.instruction("b.ne __rt_decref_object_skip");                        // if not zero, still referenced — skip free

    // -- refcount reached zero: shallow free the object --
    emitter.instruction("b __rt_heap_free");                                    // tail-call to heap free (shallow)

    emitter.label("__rt_decref_object_skip");
    emitter.instruction("ret");                                                 // return to caller
}
