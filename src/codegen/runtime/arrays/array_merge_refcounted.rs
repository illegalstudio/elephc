use crate::codegen::emit::Emitter;

/// array_merge_refcounted: merge two arrays of refcounted 8-byte payloads into a new array.
/// Input: x0 = first array pointer, x1 = second array pointer
/// Output: x0 = pointer to new merged array
pub fn emit_array_merge_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge_refcounted ---");
    emitter.label("__rt_array_merge_refcounted");

    // -- set up stack frame, save source arrays --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save first array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save second array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load first array length
    emitter.instruction("str x9, [sp, #16]");                                   // save first array length
    emitter.instruction("ldr x10, [x1]");                                       // load second array length
    emitter.instruction("str x10, [sp, #24]");                                  // save second array length

    // -- create destination array with combined capacity --
    emitter.instruction("add x0, x9, x10");                                     // compute merged capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte element slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #32]");                                   // save destination array pointer

    // -- copy first array into destination, retaining each payload --
    emitter.instruction("mov x4, #0");                                          // initialize first loop index
    emitter.label("__rt_array_merge_ref_copy1");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload first array length
    emitter.instruction("cmp x4, x9");                                          // compare index with first array length
    emitter.instruction("b.ge __rt_array_merge_ref_copy2_setup");                // move to second array when first is done
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload first array pointer
    emitter.instruction("add x2, x1, #24");                                     // compute first array data base
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // load first array element pointer
    emitter.instruction("str x5, [sp, #40]");                                   // preserve element pointer across incref call
    emitter.instruction("mov x0, x5");                                          // move element pointer into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain borrowed payload for destination ownership
    emitter.instruction("ldr x5, [sp, #40]");                                   // restore retained payload pointer
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("add x3, x0, #24");                                     // compute destination data base
    emitter.instruction("str x5, [x3, x4, lsl #3]");                            // store retained payload into destination
    emitter.instruction("add x4, x4, #1");                                      // increment first loop index
    emitter.instruction("b __rt_array_merge_ref_copy1");                        // continue copying first array

    // -- copy second array after the first segment --
    emitter.label("__rt_array_merge_ref_copy2_setup");
    emitter.instruction("mov x4, #0");                                          // initialize second loop index
    emitter.label("__rt_array_merge_ref_copy2");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload second array length
    emitter.instruction("cmp x4, x10");                                         // compare index with second array length
    emitter.instruction("b.ge __rt_array_merge_ref_done");                       // finish once second array is copied
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload second array pointer
    emitter.instruction("add x2, x1, #24");                                     // compute second array data base
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // load second array element pointer
    emitter.instruction("str x5, [sp, #40]");                                   // preserve element pointer across incref call
    emitter.instruction("mov x0, x5");                                          // move element pointer into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain borrowed payload for destination ownership
    emitter.instruction("ldr x5, [sp, #40]");                                   // restore retained payload pointer
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload first array length as destination offset
    emitter.instruction("add x6, x9, x4");                                      // compute destination index after first segment
    emitter.instruction("add x3, x0, #24");                                     // compute destination data base
    emitter.instruction("str x5, [x3, x6, lsl #3]");                            // store retained payload into destination
    emitter.instruction("add x4, x4, #1");                                      // increment second loop index
    emitter.instruction("b __rt_array_merge_ref_copy2");                        // continue copying second array

    // -- finalize destination length and return --
    emitter.label("__rt_array_merge_ref_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload first array length
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload second array length
    emitter.instruction("add x9, x9, x10");                                     // compute merged length
    emitter.instruction("str x9, [x0]");                                        // store merged length into destination header
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return merged array in x0
}
