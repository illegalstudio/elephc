use crate::codegen::emit::Emitter;

/// array_diff: return elements in arr1 that are not in arr2 (int arrays).
/// Input:  x0=arr1, x1=arr2
/// Output: x0=new array containing elements from arr1 not found in arr2
pub fn emit_array_diff(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_diff ---");
    emitter.label_global("__rt_array_diff");

    // -- set up stack frame, save arguments --
    // Stack layout:
    //   [sp, #0]  = arr1 pointer
    //   [sp, #8]  = arr2 pointer
    //   [sp, #16] = result array pointer
    //   [sp, #24] = outer loop index i
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save arr1 pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save arr2 pointer

    // -- create result array with same capacity as arr1 --
    emitter.instruction("ldr x0, [x0, #8]");                                    // x0 = arr1 capacity
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size (8 bytes per int)
    emitter.instruction("bl __rt_array_new");                                   // allocate result array, x0 = result ptr
    emitter.instruction("str x0, [sp, #16]");                                   // save result array pointer

    // -- outer loop: iterate over each element in arr1 --
    emitter.instruction("str xzr, [sp, #24]");                                  // i = 0

    emitter.label("__rt_array_diff_outer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload arr1 pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = arr1 length
    emitter.instruction("ldr x4, [sp, #24]");                                   // x4 = i
    emitter.instruction("cmp x4, x3");                                          // compare i with arr1 length
    emitter.instruction("b.ge __rt_array_diff_done");                           // if i >= length, we're done

    // -- load arr1[i] --
    emitter.instruction("add x5, x0, #24");                                     // x5 = arr1 data base
    emitter.instruction("ldr x6, [x5, x4, lsl #3]");                            // x6 = arr1[i]

    // -- inner loop: scan arr2 for this element --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload arr2 pointer
    emitter.instruction("ldr x7, [x1]");                                        // x7 = arr2 length
    emitter.instruction("add x8, x1, #24");                                     // x8 = arr2 data base
    emitter.instruction("mov x9, #0");                                          // j = 0

    emitter.label("__rt_array_diff_inner");
    emitter.instruction("cmp x9, x7");                                          // compare j with arr2 length
    emitter.instruction("b.ge __rt_array_diff_not_found");                      // if j >= length, element not in arr2

    emitter.instruction("ldr x10, [x8, x9, lsl #3]");                           // x10 = arr2[j]
    emitter.instruction("cmp x6, x10");                                         // compare arr1[i] with arr2[j]
    emitter.instruction("b.eq __rt_array_diff_found");                          // if equal, element is in arr2

    emitter.instruction("add x9, x9, #1");                                      // j += 1
    emitter.instruction("b __rt_array_diff_inner");                             // continue scanning arr2

    // -- element not found in arr2: add to result --
    emitter.label("__rt_array_diff_not_found");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result array pointer
    emitter.instruction("mov x1, x6");                                          // x1 = value to push
    emitter.instruction("bl __rt_array_push_int");                              // push element to result array

    // -- found in arr2 or pushed to result: advance outer loop --
    emitter.label("__rt_array_diff_found");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload i
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("str x4, [sp, #24]");                                   // save updated i
    emitter.instruction("b __rt_array_diff_outer");                             // continue outer loop

    // -- return result array --
    emitter.label("__rt_array_diff_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = result array
}
