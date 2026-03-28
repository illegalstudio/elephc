use crate::codegen::emit::Emitter;

/// array_fill_refcounted: create an array filled with copies of a borrowed refcounted payload.
/// Input: x0 = start_index (ignored), x1 = count, x2 = borrowed heap pointer
/// Output: x0 = pointer to new array with count retained copies of the payload
pub fn emit_array_fill_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_fill_refcounted ---");
    emitter.label("__rt_array_fill_refcounted");

    // -- set up stack frame, save count and payload --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save element count
    emitter.instruction("str x2, [sp, #8]");                                    // save borrowed payload pointer

    // -- create destination array --
    emitter.instruction("mov x0, x1");                                          // use count as destination capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #16]");                                   // save destination array pointer

    // -- append retained copies of the payload --
    emitter.instruction("mov x6, #0");                                          // initialize loop index
    emitter.label("__rt_array_fill_ref_loop");
    emitter.instruction("ldr x4, [sp, #0]");                                    // reload count
    emitter.instruction("cmp x6, x4");                                          // compare loop index with count
    emitter.instruction("b.ge __rt_array_fill_ref_done");                       // finish after pushing count elements
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload borrowed payload pointer
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained payload into destination array
    emitter.instruction("str x0, [sp, #16]");                                   // persist destination pointer after possible growth
    emitter.instruction("add x6, x6, #1");                                      // increment loop index
    emitter.instruction("b __rt_array_fill_ref_loop");                          // continue filling

    emitter.label("__rt_array_fill_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return filled array
}
