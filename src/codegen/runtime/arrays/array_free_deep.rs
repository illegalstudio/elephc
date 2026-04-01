use crate::codegen::emit::Emitter;

/// array_free_deep: free an array and release any owned heap-backed elements.
/// Input:  x0 = array pointer
/// Output: none
pub fn emit_array_free_deep(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_free_deep ---");
    emitter.label_global("__rt_array_free_deep");

    // -- null check --
    emitter.instruction("cbz x0, __rt_array_free_deep_done");                   // skip if null

    // -- heap range check (same as heap_free_safe) --
    emitter.instruction("adrp x9, _heap_buf@PAGE");                             // load page of heap buffer
    emitter.instruction("add x9, x9, _heap_buf@PAGEOFF");                       // resolve heap buffer base
    emitter.instruction("cmp x0, x9");                                          // below heap start?
    emitter.instruction("b.lo __rt_array_free_deep_done");                      // not on heap, skip
    emitter.instruction("adrp x10, _heap_off@PAGE");                            // load page of heap offset
    emitter.instruction("add x10, x10, _heap_off@PAGEOFF");                     // resolve heap offset address
    emitter.instruction("ldr x10, [x10]");                                      // current heap offset
    emitter.instruction("add x10, x9, x10");                                    // heap end = base + offset
    emitter.instruction("cmp x0, x10");                                         // beyond heap end?
    emitter.instruction("b.hs __rt_array_free_deep_done");                      // not on heap, skip

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save array pointer
    emitter.instruction("adrp x9, _gc_release_suppressed@PAGE");                // load page of the release-suppression flag
    emitter.instruction("add x9, x9, _gc_release_suppressed@PAGEOFF");          // resolve the release-suppression flag address
    emitter.instruction("mov x10, #1");                                         // ordinary deep-free walks suppress nested collector runs
    emitter.instruction("str x10, [x9]");                                       // store release-suppressed = 1 for child cleanup

    // -- load the packed runtime value_type tag for this array --
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the full kind word from the heap header
    emitter.instruction("lsr x10, x9, #8");                                     // move the packed array value_type tag into the low bits
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate the packed array value_type tag without the persistent COW flag
    emitter.instruction("cbnz x10, __rt_array_free_deep_have_tag");             // prefer the packed tag when codegen/runtime supplied one
    emitter.instruction("ldr x9, [x0, #16]");                                   // reload elem_size for older/untyped arrays
    emitter.instruction("cmp x9, #16");                                         // does this legacy array store string payloads?
    emitter.instruction("b.ne __rt_array_free_deep_struct");                    // untagged scalar arrays need no per-element cleanup
    emitter.instruction("mov x10, #1");                                         // treat legacy 16-byte arrays as string arrays
    emitter.label("__rt_array_free_deep_have_tag");
    emitter.instruction("cmp x10, #1");                                         // is this a string array?
    emitter.instruction("b.eq __rt_array_free_deep_loop_setup");                // strings release through the uniform helper
    emitter.instruction("cmp x10, #4");                                         // is this an array of indexed arrays?
    emitter.instruction("b.eq __rt_array_free_deep_loop_setup");                // nested indexed arrays need decref_any cleanup
    emitter.instruction("cmp x10, #5");                                         // is this an array of associative arrays?
    emitter.instruction("b.eq __rt_array_free_deep_loop_setup");                // nested hashes need decref_any cleanup
    emitter.instruction("cmp x10, #6");                                         // is this an array of objects / callables?
    emitter.instruction("b.eq __rt_array_free_deep_loop_setup");                // boxed mixed values need decref_any cleanup too
    emitter.instruction("cmp x10, #7");                                         // is this an array of boxed mixed values?
    emitter.instruction("b.ne __rt_array_free_deep_struct");                    // scalar arrays need no per-element cleanup

    // -- free each releasable element --
    emitter.label("__rt_array_free_deep_loop_setup");
    emitter.instruction("ldr x11, [x0]");                                       // x11 = array length
    emitter.instruction("str x11, [sp, #8]");                                   // save length
    emitter.instruction("mov x12, #0");                                         // x12 = loop index

    emitter.label("__rt_array_free_deep_loop");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload length
    emitter.instruction("cmp x12, x11");                                        // index >= length?
    emitter.instruction("b.ge __rt_array_free_deep_struct");                    // done freeing elements

    // -- load the heap-backed child pointer for this slot --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x10, [x0, #-8]");                                  // reload the full kind word from the heap header
    emitter.instruction("lsr x10, x10, #8");                                    // move the packed array value_type tag into the low bits
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate the packed array value_type tag without the persistent COW flag
    emitter.instruction("cmp x10, #1");                                         // does this array store string payloads?
    emitter.instruction("b.eq __rt_array_free_deep_load_str");                  // string payloads use 16-byte slots
    emitter.instruction("lsl x13, x12, #3");                                    // compute index * 8 for pointer-sized child slots
    emitter.instruction("add x13, x13, #24");                                   // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x13]");                                   // load the nested heap pointer from the slot
    emitter.instruction("b __rt_array_free_deep_release");                      // release pointer-sized payload through decref_any
    emitter.label("__rt_array_free_deep_load_str");
    emitter.instruction("lsl x13, x12, #4");                                    // compute index * 16 for string payload slots
    emitter.instruction("add x13, x13, #24");                                   // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x13]");                                   // load the persisted string pointer from the slot

    emitter.label("__rt_array_free_deep_release");
    emitter.instruction("str x12, [sp, #8]");                                   // save index (reuse slot, length in x10)
    emitter.instruction("bl __rt_decref_any");                                  // release the heap-backed slot payload if needed

    // -- advance --
    emitter.instruction("ldr x12, [sp, #8]");                                   // restore index
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x11, [x0]");                                       // reload length
    emitter.instruction("str x11, [sp, #8]");                                   // re-save length
    emitter.instruction("add x12, x12, #1");                                    // index += 1
    emitter.instruction("b __rt_array_free_deep_loop");                         // continue

    // -- free the array struct itself --
    emitter.label("__rt_array_free_deep_struct");
    emitter.instruction("adrp x9, _gc_release_suppressed@PAGE");                // load page of the release-suppression flag
    emitter.instruction("add x9, x9, _gc_release_suppressed@PAGEOFF");          // resolve the release-suppression flag address
    emitter.instruction("str xzr, [x9]");                                       // clear release suppression before freeing the container storage
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("bl __rt_heap_free");                                   // free array struct

    // -- restore frame --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame

    emitter.label("__rt_array_free_deep_done");
    emitter.instruction("ret");                                                 // return
}
