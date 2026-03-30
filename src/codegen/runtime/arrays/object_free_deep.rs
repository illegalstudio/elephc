use crate::codegen::emit::Emitter;

/// object_free_deep: free an object instance and release all heap-backed properties.
/// Input:  x0 = object pointer
/// Output: none
pub fn emit_object_free_deep(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_free_deep ---");
    emitter.label("__rt_object_free_deep");

    // -- null and heap-range checks --
    emitter.instruction("cbz x0, __rt_object_free_deep_done");                  // skip null objects
    emitter.instruction("adrp x9, _heap_buf@PAGE");                             // load page of the heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                       // resolve the heap buffer base
    emitter.instruction("cmp x0, x9");                                          // is the object below the heap buffer?
    emitter.instruction("b.lo __rt_object_free_deep_done");                     // skip non-heap pointers
    emitter.instruction("adrp x10, _heap_off@PAGE");                            // load page of the heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                     // resolve the heap offset address
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the object at or beyond the heap end?
    emitter.instruction("b.hs __rt_object_free_deep_done");                     // skip invalid pointers

    // -- set up stack frame --
    // Stack layout:
    //   [sp, #0]  = object pointer
    //   [sp, #8]  = descriptor pointer
    //   [sp, #16] = property count
    //   [sp, #24] = loop index
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame for object cleanup
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer
    emitter.instruction("adrp x9, _gc_release_suppressed@PAGE");                // load page of the release-suppression flag
    emitter.instruction("add x9, x9, _gc_release_suppressed@PAGEOFF");          // resolve the release-suppression flag address
    emitter.instruction("mov x10, #1");                                         // ordinary deep-free walks suppress nested collector runs
    emitter.instruction("str x10, [x9]");                                       // store release-suppressed = 1 for child cleanup

    // -- derive property count from the object payload size --
    emitter.instruction("ldr w9, [x0, #-16]");                                  // load the object payload size from the heap header
    emitter.instruction("sub x9, x9, #8");                                      // subtract the leading class_id field
    emitter.instruction("lsr x9, x9, #4");                                      // divide by 16 to get the number of property slots
    emitter.instruction("str x9, [sp, #16]");                                   // save the property count for the cleanup loop

    // -- resolve the per-class property tag descriptor --
    emitter.instruction("ldr x10, [x0]");                                       // load the runtime class_id from the object payload
    emitter.instruction("adrp x11, _class_gc_desc_count@PAGE");                 // load page of the descriptor count table
    emitter.instruction("add x11, x11, _class_gc_desc_count@PAGEOFF");          // resolve the descriptor count address
    emitter.instruction("ldr x11, [x11]");                                      // load the number of emitted class descriptors
    emitter.instruction("cmp x10, x11");                                        // is class_id within the descriptor table?
    emitter.instruction("b.hs __rt_object_free_deep_struct");                   // invalid class ids fall back to a shallow free
    emitter.instruction("adrp x11, _class_gc_desc_ptrs@PAGE");                  // load page of the descriptor pointer table
    emitter.instruction("add x11, x11, _class_gc_desc_ptrs@PAGEOFF");           // resolve the descriptor pointer table
    emitter.instruction("lsl x12, x10, #3");                                    // scale class_id by 8 bytes per descriptor pointer
    emitter.instruction("ldr x11, [x11, x12]");                                 // load the tag descriptor pointer for this class
    emitter.instruction("str x11, [sp, #8]");                                   // save descriptor pointer for the cleanup loop
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize property index = 0

    // -- walk each property and release heap-backed values based on the descriptor tags --
    emitter.label("__rt_object_free_deep_loop");
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload the current property index
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the total property count
    emitter.instruction("cmp x12, x13");                                        // have we visited every property slot?
    emitter.instruction("b.ge __rt_object_free_deep_struct");                   // finish once every property has been scanned

    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("mov x10, #16");                                        // each property slot occupies 16 bytes
    emitter.instruction("mul x10, x12, x10");                                   // compute the property slot byte offset
    emitter.instruction("add x10, x10, #8");                                    // skip the leading class_id field
    emitter.instruction("ldr x14, [x9, x10]");                                  // load the low 8-byte payload for this property
    emitter.instruction("add x11, x10, #8");                                    // compute the offset of the metadata / length word
    emitter.instruction("ldr x15, [x9, x11]");                                  // load the runtime metadata / length word for this property
    emitter.instruction("cmp x15, #4");                                         // was this property last written with an indexed array?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // runtime metadata takes priority for refcounted payloads
    emitter.instruction("cmp x15, #5");                                         // was this property last written with an associative array?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // runtime metadata takes priority for refcounted payloads
    emitter.instruction("cmp x15, #6");                                         // was this property last written with an object?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // runtime metadata takes priority for refcounted payloads
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the descriptor pointer
    emitter.instruction("ldrb w15, [x11, x12]");                                // load the compile-time fallback tag for this property slot
    emitter.instruction("cmp x15, #1");                                         // is this a compile-time string property?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // strings release through the uniform dispatch helper
    emitter.instruction("b __rt_object_free_deep_next");                        // scalars and nulls need no cleanup

    emitter.label("__rt_object_free_deep_release_runtime");
    emitter.instruction("mov x0, x14");                                         // move the property payload pointer into the uniform release helper arg reg
    emitter.instruction("str x12, [sp, #24]");                                  // preserve the property index across the helper call
    emitter.instruction("bl __rt_decref_any");                                  // release the heap-backed property payload if needed
    emitter.instruction("ldr x12, [sp, #24]");                                  // restore the property index after the helper call

    emitter.label("__rt_object_free_deep_next");
    emitter.instruction("add x12, x12, #1");                                    // advance to the next property slot
    emitter.instruction("str x12, [sp, #24]");                                  // save the updated property index
    emitter.instruction("b __rt_object_free_deep_loop");                        // continue scanning property slots

    // -- free the object storage itself --
    emitter.label("__rt_object_free_deep_struct");
    emitter.instruction("adrp x9, _gc_release_suppressed@PAGE");                // load page of the release-suppression flag
    emitter.instruction("add x9, x9, _gc_release_suppressed@PAGEOFF");          // resolve the release-suppression flag address
    emitter.instruction("str xzr, [x9]");                                       // clear release suppression before freeing the object storage
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer before freeing it
    emitter.instruction("bl __rt_heap_free");                                   // return the object storage to the heap allocator
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // tear down the object cleanup stack frame

    emitter.label("__rt_object_free_deep_done");
    emitter.instruction("ret");                                                 // return to the caller
}
