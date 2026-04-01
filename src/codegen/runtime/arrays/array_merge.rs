use crate::codegen::emit::Emitter;

/// array_merge: merge two integer arrays into a new array.
/// Input: x0 = first array pointer, x1 = second array pointer
/// Output: x0 = pointer to new merged array
pub fn emit_array_merge(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge ---");
    emitter.label_global("__rt_array_merge");

    // -- set up stack frame, save source arrays --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save arr1 pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save arr2 pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = arr1 length
    emitter.instruction("str x9, [sp, #16]");                                   // save arr1 length
    emitter.instruction("ldr x10, [x1]");                                       // x10 = arr2 length
    emitter.instruction("str x10, [sp, #24]");                                  // save arr2 length

    // -- create new array with combined capacity --
    emitter.instruction("add x0, x9, x10");                                     // x0 = total capacity = len1 + len2
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array, x0 = new array ptr
    emitter.instruction("str x0, [sp, #32]");                                   // save new array pointer

    // -- copy arr1 elements to new array --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = arr1 pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = arr1 length
    emitter.instruction("add x2, x1, #24");                                     // x2 = arr1 data base
    emitter.instruction("add x3, x0, #24");                                     // x3 = new array data base
    emitter.instruction("mov x4, #0");                                          // x4 = i = 0

    emitter.label("__rt_array_merge_copy1");
    emitter.instruction("cmp x4, x9");                                          // compare i with arr1 length
    emitter.instruction("b.ge __rt_array_merge_copy2_setup");                   // if done with arr1, move to arr2
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // x5 = arr1[i]
    emitter.instruction("str x5, [x3, x4, lsl #3]");                            // new_arr[i] = arr1[i]
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("b __rt_array_merge_copy1");                            // continue loop

    // -- copy arr2 elements after arr1 elements --
    emitter.label("__rt_array_merge_copy2_setup");
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = arr2 pointer
    emitter.instruction("ldr x10, [sp, #24]");                                  // x10 = arr2 length
    emitter.instruction("add x2, x1, #24");                                     // x2 = arr2 data base
    emitter.instruction("ldr x0, [sp, #32]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = arr1 length (offset for writing)
    emitter.instruction("add x3, x0, #24");                                     // x3 = new array data base
    emitter.instruction("mov x4, #0");                                          // x4 = j = 0

    emitter.label("__rt_array_merge_copy2");
    emitter.instruction("cmp x4, x10");                                         // compare j with arr2 length
    emitter.instruction("b.ge __rt_array_merge_done");                          // if done with arr2, finish up
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // x5 = arr2[j]
    emitter.instruction("add x6, x9, x4");                                      // x6 = arr1_len + j (destination index)
    emitter.instruction("str x5, [x3, x6, lsl #3]");                            // new_arr[arr1_len + j] = arr2[j]
    emitter.instruction("add x4, x4, #1");                                      // j += 1
    emitter.instruction("b __rt_array_merge_copy2");                            // continue loop

    // -- set length on new array and return --
    emitter.label("__rt_array_merge_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = arr1 length
    emitter.instruction("ldr x10, [sp, #24]");                                  // x10 = arr2 length
    emitter.instruction("add x9, x9, x10");                                     // x9 = total length
    emitter.instruction("str x9, [x0]");                                        // set new array length

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = merged array
}
