use crate::codegen::emit::Emitter;

/// strtoupper: copy string to concat_buf, uppercasing a-z.
pub fn emit_strtoupper(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtoupper ---");
    emitter.label_global("__rt_strtoupper");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- get concat_buf write position --
    emitter.adrp("x6", "_concat_off");                           // load page address of concat buffer offset
    emitter.add_lo12("x6", "x6", "_concat_off");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.adrp("x7", "_concat_buf");                           // load page address of concat buffer
    emitter.add_lo12("x7", "x7", "_concat_buf");                     // resolve exact buffer base address
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("mov x10, x9");                                         // save destination start for return value
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter

    // -- copy bytes, converting lowercase to uppercase --
    emitter.label("__rt_strtoupper_loop");
    emitter.instruction("cbz x11, __rt_strtoupper_done");                       // if no bytes remain, done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance ptr
    emitter.instruction("cmp w12, #97");                                        // compare with 'a' (0x61)
    emitter.instruction("b.lt __rt_strtoupper_store");                          // if below 'a', store unchanged
    emitter.instruction("cmp w12, #122");                                       // compare with 'z' (0x7A)
    emitter.instruction("b.gt __rt_strtoupper_store");                          // if above 'z', store unchanged
    emitter.instruction("sub w12, w12, #32");                                   // convert a-z to A-Z by subtracting 32
    emitter.label("__rt_strtoupper_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store (possibly uppered) byte, advance dest
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining count
    emitter.instruction("b __rt_strtoupper_loop");                              // continue processing next byte

    // -- update concat_off and return --
    emitter.label("__rt_strtoupper_done");
    emitter.instruction("add x8, x8, x2");                                      // advance offset by string length
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x1, x10");                                         // return new pointer (start of uppered copy)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
