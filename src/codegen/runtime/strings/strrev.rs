use crate::codegen::emit::Emitter;

/// strrev: reverse a string into concat_buf.
pub fn emit_strrev(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strrev ---");
    emitter.label_global("__rt_strrev");

    // -- get concat_buf write position --
    emitter.adrp("x6", "_concat_off");                           // load page address of concat buffer offset
    emitter.add_lo12("x6", "x6", "_concat_off");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.adrp("x7", "_concat_buf");                           // load page address of concat buffer
    emitter.add_lo12("x7", "x7", "_concat_buf");                     // resolve exact buffer base address
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("mov x10, x9");                                         // save destination start for return value
    emitter.instruction("add x11, x1, x2");                                     // x11 = pointer to end of source string
    emitter.instruction("mov x12, x2");                                         // copy length as loop counter

    // -- copy bytes in reverse order (last-to-first) --
    emitter.label("__rt_strrev_loop");
    emitter.instruction("cbz x12, __rt_strrev_done");                           // if no bytes remain, done reversing
    emitter.instruction("sub x11, x11, #1");                                    // move source pointer backward (from end)
    emitter.instruction("ldrb w13, [x11]");                                     // load byte from current source position
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest (forward order), advance dest
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_strrev_loop");                                  // continue reversing

    // -- update concat_off and return --
    emitter.label("__rt_strrev_done");
    emitter.instruction("add x8, x8, x2");                                      // advance offset by string length
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x1, x10");                                         // return pointer to reversed string
    // x2 unchanged
    emitter.instruction("ret");                                                 // return to caller
}
