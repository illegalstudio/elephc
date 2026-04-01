use crate::codegen::emit::Emitter;

/// array_diff_refcounted: return elements in arr1 that are not in arr2 for refcounted payload arrays.
/// Input:  x0=arr1, x1=arr2
/// Output: x0=new array containing retained elements from arr1 not found in arr2
pub fn emit_array_diff_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_diff_refcounted ---");
    emitter.label_global("__rt_array_diff_refcounted");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save first array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save second array pointer

    // -- create result array with same capacity as arr1 --
    emitter.instruction("ldr x0, [x0, #8]");                                    // load first array capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate result array
    emitter.instruction("str x0, [sp, #16]");                                   // save result array pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize outer loop index

    emitter.label("__rt_array_diff_ref_outer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload first array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load first array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload outer loop index
    emitter.instruction("cmp x4, x3");                                          // compare index with first array length
    emitter.instruction("b.ge __rt_array_diff_ref_done");                       // finish once every source element has been considered
    emitter.instruction("add x5, x0, #24");                                     // compute first array data base
    emitter.instruction("ldr x6, [x5, x4, lsl #3]");                            // load candidate payload from first array
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload second array pointer
    emitter.instruction("ldr x7, [x1]");                                        // load second array length
    emitter.instruction("add x8, x1, #24");                                     // compute second array data base
    emitter.instruction("mov x9, #0");                                          // initialize inner loop index

    emitter.label("__rt_array_diff_ref_inner");
    emitter.instruction("cmp x9, x7");                                          // compare inner index with second array length
    emitter.instruction("b.ge __rt_array_diff_ref_not_found");                  // candidate is unique if scan reaches the end
    emitter.instruction("ldr x10, [x8, x9, lsl #3]");                           // load second array payload
    emitter.instruction("cmp x6, x10");                                         // compare pointer identity
    emitter.instruction("b.eq __rt_array_diff_ref_found");                      // skip candidate when the payload already exists in arr2
    emitter.instruction("add x9, x9, #1");                                      // increment inner loop index
    emitter.instruction("b __rt_array_diff_ref_inner");                         // continue scanning the second array

    emitter.label("__rt_array_diff_ref_not_found");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload result array pointer
    emitter.instruction("mov x1, x6");                                          // move borrowed candidate payload into push argument register
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained candidate to the result array
    emitter.instruction("str x0, [sp, #16]");                                   // persist result pointer after possible growth

    emitter.label("__rt_array_diff_ref_found");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload outer loop index
    emitter.instruction("add x4, x4, #1");                                      // increment outer loop index
    emitter.instruction("str x4, [sp, #24]");                                   // persist updated outer loop index
    emitter.instruction("b __rt_array_diff_ref_outer");                         // continue scanning arr1

    emitter.label("__rt_array_diff_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload result array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return result array
}
