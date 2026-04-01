use crate::codegen::emit::Emitter;

/// array_reverse_refcounted: create a reversed copy of a refcounted array.
/// Input: x0 = array pointer
/// Output: x0 = pointer to new reversed array
pub fn emit_array_reverse_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_reverse_refcounted ---");
    emitter.label_global("__rt_array_reverse_refcounted");

    // -- set up stack frame, save source array info --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save source length

    // -- create destination array with matching capacity --
    emitter.instruction("mov x0, x9");                                          // use source length as destination capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #16]");                                   // save destination array pointer

    // -- iterate backwards and append retained payloads --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload source length after allocator call
    emitter.instruction("sub x4, x9, #1");                                      // initialize source index to the last element
    emitter.label("__rt_array_reverse_ref_loop");
    emitter.instruction("cmp x4, #0");                                          // test whether source index dropped below zero
    emitter.instruction("b.lt __rt_array_reverse_ref_done");                    // finish when every source element has been copied
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x2, x1, #24");                                     // compute source data base
    emitter.instruction("ldr x1, [x2, x4, lsl #3]");                            // load borrowed source payload
    emitter.instruction("str x4, [sp, #24]");                                   // preserve source index across helper calls
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained payload into destination array
    emitter.instruction("str x0, [sp, #16]");                                   // persist destination pointer after possible growth
    emitter.instruction("ldr x4, [sp, #24]");                                   // restore source index after helper calls
    emitter.instruction("sub x4, x4, #1");                                      // decrement source index
    emitter.instruction("b __rt_array_reverse_ref_loop");                       // continue reversing

    emitter.label("__rt_array_reverse_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return reversed array
}
