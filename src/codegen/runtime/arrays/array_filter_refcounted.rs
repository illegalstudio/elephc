use crate::codegen::emit::Emitter;

/// array_filter_refcounted: filter a refcounted array using a callback, returning a new array.
/// Input: x0 = callback function address, x1 = source array pointer
/// Output: x0 = pointer to new filtered array
pub fn emit_array_filter_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_filter_refcounted ---");
    emitter.label("__rt_array_filter_refcounted");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #48]");                             // save callee-saved x19 and x20
    emitter.instruction("str x21, [sp, #40]");                                  // save callee-saved x21
    emitter.instruction("str x0, [sp, #0]");                                    // save callback address
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer
    emitter.instruction("mov x19, x0");                                         // keep callback address in callee-saved register

    // -- read source length and create destination array --
    emitter.instruction("ldr x9, [x1]");                                        // load source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save source array length
    emitter.instruction("mov x0, x9");                                          // use source length as destination capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #24]");                                   // save destination array pointer
    emitter.instruction("mov x20, #0");                                         // initialize source index
    emitter.instruction("mov x21, #0");                                         // initialize destination length tracker

    emitter.label("__rt_array_filter_ref_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload source length
    emitter.instruction("cmp x20, x9");                                         // compare source index with source length
    emitter.instruction("b.ge __rt_array_filter_ref_done");                     // finish once every source element has been examined
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("add x1, x1, #24");                                     // compute source data base
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // load source element for callback
    emitter.instruction("str x0, [sp, #32]");                                   // preserve source element across callback
    emitter.instruction("blr x19");                                             // call callback with source element in x0
    emitter.instruction("cbz x0, __rt_array_filter_ref_skip");                  // skip element when callback returned falsy
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload borrowed source payload
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained payload into destination array
    emitter.instruction("str x0, [sp, #24]");                                   // persist destination pointer after possible growth
    emitter.instruction("add x21, x21, #1");                                    // track number of kept elements

    emitter.label("__rt_array_filter_ref_skip");
    emitter.instruction("add x20, x20, #1");                                    // increment source index
    emitter.instruction("b __rt_array_filter_ref_loop");                        // continue filtering

    emitter.label("__rt_array_filter_ref_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload destination array pointer
    emitter.instruction("str x21, [x0]");                                       // set filtered array length
    emitter.instruction("ldr x21, [sp, #40]");                                  // restore callee-saved x21
    emitter.instruction("ldp x19, x20, [sp, #48]");                             // restore callee-saved x19 and x20
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return filtered array
}
