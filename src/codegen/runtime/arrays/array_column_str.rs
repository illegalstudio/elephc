use crate::codegen::emit::Emitter;

/// array_column_str: extract a string column from an array of associative arrays.
/// Input: x0=outer array (Array of AssocArray), x1=column key ptr, x2=column key len
/// Output: x0=new array containing the string column values (elem_size=16)
pub fn emit_array_column_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_column_str ---");
    emitter.label("__rt_array_column_str");

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

    // -- create result array with elem_size=16 for string values --
    emitter.instruction("mov x0, x9");                                          // capacity = outer length
    emitter.instruction("mov x1, #16");                                         // element size = 16 (string ptr+len)
    emitter.instruction("bl __rt_array_new");                                   // create result array
    emitter.instruction("str x0, [sp, #32]");                                   // save result array pointer

    // -- iterate outer array --
    emitter.instruction("str xzr, [sp, #40]");                                  // loop index = 0

    emitter.label("__rt_acs_loop");
    emitter.instruction("ldr x9, [sp, #40]");                                   // load current index
    emitter.instruction("ldr x10, [sp, #24]");                                  // load outer length
    emitter.instruction("cmp x9, x10");                                         // compare index with length
    emitter.instruction("b.ge __rt_acs_done");                                  // if done, exit loop

    // -- load inner assoc array pointer at index --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload outer array
    emitter.instruction("add x0, x0, #24");                                     // skip header
    emitter.instruction("ldr x0, [x0, x9, lsl #3]");                            // load inner hash table pointer at index

    // -- look up column key in inner hash table --
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // reload column key ptr/len
    emitter.instruction("bl __rt_hash_get");                                    // lookup -> x0=found, x1=val_lo(str_ptr), x2=val_hi(str_len)

    // -- if found, push string value to result array --
    emitter.instruction("cbz x0, __rt_acs_skip");                               // skip if key not found

    // -- push string value to result array --
    emitter.instruction("mov x3, x1");                                          // save string pointer
    emitter.instruction("mov x4, x2");                                          // save string length
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload result array
    emitter.instruction("mov x1, x3");                                          // string pointer as arg
    emitter.instruction("mov x2, x4");                                          // string length as arg
    emitter.instruction("bl __rt_array_push_str");                              // push string to result array
    emitter.instruction("str x0, [sp, #32]");                                   // update array pointer after possible realloc

    emitter.label("__rt_acs_skip");
    // -- increment index --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload index
    emitter.instruction("add x9, x9, #1");                                      // increment
    emitter.instruction("str x9, [sp, #40]");                                   // save updated index
    emitter.instruction("b __rt_acs_loop");                                     // continue loop

    emitter.label("__rt_acs_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // return result array

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
