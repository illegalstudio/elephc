use crate::codegen::emit::Emitter;

/// stripslashes: remove escape backslashes.
/// Input: x1/x2=string. Output: x1/x2=result.
pub fn emit_stripslashes(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stripslashes ---");
    emitter.label("__rt_stripslashes");

    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_stripslashes_loop");
    emitter.instruction("cbz x11, __rt_stripslashes_done");                     // done if no bytes left
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.instruction("cmp w12, #92");                                        // is it a backslash?
    emitter.instruction("b.ne __rt_stripslashes_store");                        // no → store as-is
    // -- backslash: skip it and store the next char --
    emitter.instruction("cbz x11, __rt_stripslashes_store");                    // trailing backslash → store it
    emitter.instruction("ldrb w12, [x1], #1");                                  // load escaped char, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.label("__rt_stripslashes_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to output
    emitter.instruction("b __rt_stripslashes_loop");                            // next byte

    emitter.label("__rt_stripslashes_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}
