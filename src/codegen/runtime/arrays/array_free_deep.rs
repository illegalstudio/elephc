use crate::codegen::emit::Emitter;

/// array_free_deep: free an array AND all its string elements.
/// For string arrays (elem_size=16), iterates over elements and frees each
/// string pointer before freeing the array struct itself.
/// For non-string arrays (elem_size=8), just frees the array struct.
/// Input:  x0 = array pointer
/// Output: none
pub fn emit_array_free_deep(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_free_deep ---");
    emitter.label("__rt_array_free_deep");

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

    // -- check if this is a string array (elem_size == 16) --
    emitter.instruction("ldr x9, [x0, #16]");                                   // x9 = elem_size
    emitter.instruction("cmp x9, #16");                                         // is this a string array?
    emitter.instruction("b.ne __rt_array_free_deep_struct");                    // no — skip element freeing

    // -- free each string element --
    emitter.instruction("ldr x10, [x0]");                                       // x10 = array length
    emitter.instruction("str x10, [sp, #8]");                                   // save length
    emitter.instruction("add x11, x0, #24");                                    // x11 = data region start
    emitter.instruction("mov x12, #0");                                         // x12 = loop index

    emitter.label("__rt_array_free_deep_loop");
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload length
    emitter.instruction("cmp x12, x10");                                        // index >= length?
    emitter.instruction("b.ge __rt_array_free_deep_struct");                    // done freeing elements

    // -- load string pointer at data[index * 16] --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("add x11, x0, #24");                                    // data region
    emitter.instruction("lsl x13, x12, #4");                                    // index * 16
    emitter.instruction("ldr x0, [x11, x13]");                                  // x0 = string pointer
    emitter.instruction("str x12, [sp, #8]");                                   // save index (reuse slot, length in x10)

    // -- free the string (safe: handles null, .data, garbage) --
    emitter.instruction("bl __rt_heap_free_safe");                              // free string element

    // -- advance --
    emitter.instruction("ldr x12, [sp, #8]");                                   // restore index
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x10, [x0]");                                       // reload length
    emitter.instruction("str x10, [sp, #8]");                                   // re-save length
    emitter.instruction("add x12, x12, #1");                                    // index += 1
    emitter.instruction("b __rt_array_free_deep_loop");                         // continue

    // -- free the array struct itself --
    emitter.label("__rt_array_free_deep_struct");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("bl __rt_heap_free");                                   // free array struct

    // -- restore frame --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame

    emitter.label("__rt_array_free_deep_done");
    emitter.instruction("ret");                                                 // return
}
