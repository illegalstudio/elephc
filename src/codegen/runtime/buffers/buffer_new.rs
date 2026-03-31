use crate::codegen::emit::Emitter;

pub fn emit_buffer_new(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_new ---");
    emitter.label("__rt_buffer_new");

    // -- save len/stride across heap allocation --
    emitter.instruction("sub sp, sp, #32");                                     // allocate a small stack frame for saved arguments
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the temporary frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save requested logical length
    emitter.instruction("str x1, [sp, #8]");                                    // save requested element stride

    // -- allocate header + contiguous payload --
    emitter.instruction("mul x2, x0, x1");                                      // compute payload byte count = len * stride
    emitter.instruction("add x0, x2, #16");                                     // add the 16-byte buffer header
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the full buffer payload on the shared heap

    // -- initialize header fields --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the logical length after allocation
    emitter.instruction("str x9, [x0]");                                        // header[0] = logical element count
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the element stride after allocation
    emitter.instruction("str x9, [x0, #8]");                                    // header[8] = element stride in bytes

    // -- zero-initialize the contiguous payload --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the logical length for payload size calculation
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the stride for payload size calculation
    emitter.instruction("mul x12, x9, x10");                                    // compute payload byte count = len * stride
    emitter.instruction("add x11, x0, #16");                                    // x11 = first payload byte after the 16-byte header
    emitter.instruction("add x12, x11, x12");                                   // x12 = end pointer one past the payload
    emitter.label("__rt_buffer_new_zero_loop");
    emitter.instruction("cmp x11, x12");                                        // have we cleared the whole payload yet?
    emitter.instruction("b.eq __rt_buffer_new_zero_done");                      // yes — skip the zero-fill loop
    emitter.instruction("str xzr, [x11], #8");                                  // store one zeroed machine word and advance
    emitter.instruction("b __rt_buffer_new_zero_loop");                         // continue zeroing until the end pointer is reached
    emitter.label("__rt_buffer_new_zero_done");

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the temporary frame
    emitter.instruction("ret");                                                 // return x0 = buffer header pointer
}
