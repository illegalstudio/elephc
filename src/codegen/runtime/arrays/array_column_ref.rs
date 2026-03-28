use crate::codegen::emit::Emitter;

/// array_column_ref: extract a refcounted column from an array of associative arrays.
/// Input: x0=outer array (Array of AssocArray), x1=column key ptr, x2=column key len
/// Output: x0=new array containing retained heap pointers (elem_size=8)
pub fn emit_array_column_ref(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_column_ref ---");
    emitter.label("__rt_array_column_ref");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set frame pointer

    // -- save inputs --
    emitter.instruction("str x0, [sp, #0]");                                    // save outer array pointer
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save column key ptr/len

    // -- load outer array length --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = outer array length
    emitter.instruction("str x9, [sp, #24]");                                   // save outer length

    // -- create result array with elem_size=8 for heap pointers --
    emitter.instruction("mov x0, x9");                                          // capacity = outer length
    emitter.instruction("mov x1, #8");                                          // element size = 8 (heap pointer)
    emitter.instruction("bl __rt_array_new");                                   // create result array
    emitter.instruction("str x0, [sp, #32]");                                   // save result array pointer

    // -- iterate outer array --
    emitter.instruction("str xzr, [sp, #40]");                                  // loop index = 0

    emitter.label("__rt_acr_loop");
    emitter.instruction("ldr x9, [sp, #40]");                                   // load current index
    emitter.instruction("ldr x10, [sp, #24]");                                  // load outer length
    emitter.instruction("cmp x9, x10");                                         // compare index with length
    emitter.instruction("b.ge __rt_acr_done");                                  // if done, exit loop

    // -- load inner assoc array pointer at index --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload outer array
    emitter.instruction("add x0, x0, #24");                                     // skip header
    emitter.instruction("ldr x0, [x0, x9, lsl #3]");                            // load inner hash table pointer at index

    // -- look up column key in inner hash table --
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // reload column key ptr/len
    emitter.instruction("bl __rt_hash_get");                                    // lookup → x0=found, x1=val_lo(heap ptr), x2=val_hi

    // -- if found, retain and push heap value to result array --
    emitter.instruction("cbz x0, __rt_acr_skip");                               // skip if key not found
    emitter.instruction("str x1, [sp, #-16]!");                                 // save borrowed heap pointer across retain and push calls
    emitter.instruction("ldr x0, [sp]");                                        // reload borrowed heap pointer for incref
    emitter.instruction("bl __rt_incref");                                      // retain copied heap value for the result array
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload result array pointer (sp+16+32)
    emitter.instruction("ldr x1, [sp]");                                        // reload retained heap pointer
    emitter.instruction("bl __rt_array_push_int");                              // push heap pointer into result array
    emitter.instruction("add sp, sp, #16");                                     // drop saved heap pointer

    emitter.label("__rt_acr_skip");
    // -- increment index --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload index
    emitter.instruction("add x9, x9, #1");                                      // increment
    emitter.instruction("str x9, [sp, #40]");                                   // save updated index
    emitter.instruction("b __rt_acr_loop");                                     // continue loop

    emitter.label("__rt_acr_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // return result array

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
