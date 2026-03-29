use crate::codegen::emit::Emitter;

/// hash_free_deep: free a hash table and all owned keys / heap-backed values.
/// Input:  x0 = hash table pointer
/// Output: none
pub fn emit_hash_free_deep(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_free_deep ---");
    emitter.label("__rt_hash_free_deep");

    // -- null and heap-range check --
    emitter.instruction("cbz x0, __rt_hash_free_deep_done");                    // skip if null
    emitter.instruction("adrp x9, _heap_buf@PAGE");                             // load page of heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                       // resolve heap buffer base
    emitter.instruction("cmp x0, x9");                                          // is table below heap start?
    emitter.instruction("b.lo __rt_hash_free_deep_done");                       // skip non-heap pointers
    emitter.instruction("adrp x10, _heap_off@PAGE");                            // load page of heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                     // resolve heap offset address
    emitter.instruction("ldr x10, [x10]");                                      // load current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute current heap end
    emitter.instruction("cmp x0, x10");                                         // is table at or beyond heap end?
    emitter.instruction("b.hs __rt_hash_free_deep_done");                       // skip invalid pointers

    // -- set up stack frame --
    // Stack layout:
    //   [sp, #0]  = hash table pointer
    //   [sp, #8]  = capacity
    //   [sp, #16] = value_type
    //   [sp, #24] = loop index
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save hash table pointer
    emitter.instruction("adrp x9, _gc_release_suppressed@PAGE");                // load page of the release-suppression flag
    emitter.instruction("add x9, x9, _gc_release_suppressed@PAGEOFF");          // resolve the release-suppression flag address
    emitter.instruction("mov x10, #1");                                         // ordinary deep-free walks suppress nested collector runs
    emitter.instruction("str x10, [x9]");                                       // store release-suppressed = 1 for child cleanup
    emitter.instruction("ldr x9, [x0, #8]");                                    // load table capacity
    emitter.instruction("str x9, [sp, #8]");                                    // save capacity for the loop
    emitter.instruction("ldr x9, [x0, #16]");                                   // load runtime value_type tag
    emitter.instruction("str x9, [sp, #16]");                                   // save value_type for cleanup dispatch
    emitter.instruction("str xzr, [sp, #24]");                                  // loop index = 0

    // -- iterate all slots and free occupied entries --
    emitter.label("__rt_hash_free_deep_loop");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload loop index
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload capacity
    emitter.instruction("cmp x11, x10");                                        // are we done scanning all slots?
    emitter.instruction("b.ge __rt_hash_free_deep_struct");                     // finish once index reaches capacity

    emitter.instruction("ldr x9, [sp, #0]");                                    // reload hash table pointer
    emitter.instruction("mov x12, #40");                                        // entry size = 40 bytes
    emitter.instruction("mul x13, x11, x12");                                   // compute byte offset for this slot
    emitter.instruction("add x13, x9, x13");                                    // advance from table base to slot
    emitter.instruction("add x13, x13, #24");                                   // skip hash header to entry storage
    emitter.instruction("ldr x14, [x13]");                                      // load occupied flag
    emitter.instruction("cmp x14, #1");                                         // is this slot occupied?
    emitter.instruction("b.ne __rt_hash_free_deep_next");                       // skip empty or tombstone slots

    // -- free the owned key string for this entry --
    emitter.instruction("ldr x0, [x13, #8]");                                   // load owned key pointer
    emitter.instruction("str x11, [sp, #24]");                                  // preserve loop index across helper call
    emitter.instruction("bl __rt_heap_free_safe");                              // free persisted key storage
    emitter.instruction("ldr x11, [sp, #24]");                                  // restore loop index after helper call

    // -- free the entry value based on the runtime value tag --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload hash table pointer after helper call
    emitter.instruction("mov x12, #40");                                        // entry size = 40 bytes
    emitter.instruction("mul x13, x11, x12");                                   // recompute byte offset for this slot
    emitter.instruction("add x13, x9, x13");                                    // advance from table base to slot
    emitter.instruction("add x13, x13, #24");                                   // skip hash header to entry storage
    emitter.instruction("ldr x14, [sp, #16]");                                  // reload runtime value_type tag
    emitter.instruction("cmp x14, #1");                                         // is the entry value heap-backed at all?
    emitter.instruction("b.eq __rt_hash_free_deep_value_any");                  // strings release through the uniform dispatch helper
    emitter.instruction("cmp x14, #4");                                         // is this a nested indexed array?
    emitter.instruction("b.eq __rt_hash_free_deep_value_any");                  // arrays release through the uniform dispatch helper
    emitter.instruction("cmp x14, #5");                                         // is this a nested associative array?
    emitter.instruction("b.eq __rt_hash_free_deep_value_any");                  // hashes release through the uniform dispatch helper
    emitter.instruction("cmp x14, #6");                                         // is this a nested object / callable?
    emitter.instruction("b.eq __rt_hash_free_deep_value_any");                  // objects release through the uniform dispatch helper
    emitter.instruction("b __rt_hash_free_deep_next");                          // plain scalars need no cleanup

    emitter.label("__rt_hash_free_deep_value_any");
    emitter.instruction("ldr x0, [x13, #24]");                                  // load the heap-backed value pointer from the entry payload
    emitter.instruction("str x11, [sp, #24]");                                  // preserve loop index across helper call
    emitter.instruction("bl __rt_decref_any");                                  // release the heap-backed value through the uniform dispatcher
    emitter.instruction("ldr x11, [sp, #24]");                                  // restore loop index after helper call

    emitter.label("__rt_hash_free_deep_next");
    emitter.instruction("add x11, x11, #1");                                    // advance to the next slot
    emitter.instruction("str x11, [sp, #24]");                                  // save updated loop index
    emitter.instruction("b __rt_hash_free_deep_loop");                          // continue scanning entries

    // -- free the hash table struct itself --
    emitter.label("__rt_hash_free_deep_struct");
    emitter.instruction("adrp x9, _gc_release_suppressed@PAGE");                // load page of the release-suppression flag
    emitter.instruction("add x9, x9, _gc_release_suppressed@PAGEOFF");          // resolve the release-suppression flag address
    emitter.instruction("str xzr, [x9]");                                       // clear release suppression before freeing the container storage
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload hash table pointer
    emitter.instruction("bl __rt_heap_free");                                   // free the hash table storage
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame

    emitter.label("__rt_hash_free_deep_done");
    emitter.instruction("ret");                                                 // return to caller
}
