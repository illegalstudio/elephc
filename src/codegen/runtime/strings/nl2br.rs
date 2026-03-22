use crate::codegen::emit::Emitter;

/// nl2br: insert "<br />\n" before each newline.
/// Input: x1/x2=string. Output: x1/x2=result.
pub fn emit_nl2br(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: nl2br ---");
    emitter.label("__rt_nl2br");

    emitter.instruction("adrp x6, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                          // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                    // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining count

    emitter.label("__rt_nl2br_loop");
    emitter.instruction("cbz x11, __rt_nl2br_done");                            // no bytes left → done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.instruction("cmp w12, #10");                                        // is it '\n'?
    emitter.instruction("b.ne __rt_nl2br_store");                               // no → store as-is
    // -- insert "<br />" before the newline --
    emitter.instruction("mov w13, #60");                                        // '<'
    emitter.instruction("strb w13, [x9], #1");                                  // write '<'
    emitter.instruction("mov w13, #98");                                        // 'b'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'b'
    emitter.instruction("mov w13, #114");                                       // 'r'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'r'
    emitter.instruction("mov w13, #32");                                        // ' '
    emitter.instruction("strb w13, [x9], #1");                                  // write ' '
    emitter.instruction("mov w13, #47");                                        // '/'
    emitter.instruction("strb w13, [x9], #1");                                  // write '/'
    emitter.instruction("mov w13, #62");                                        // '>'
    emitter.instruction("strb w13, [x9], #1");                                  // write '>'
    emitter.label("__rt_nl2br_store");
    emitter.instruction("strb w12, [x9], #1");                                  // write original byte (including '\n')
    emitter.instruction("b __rt_nl2br_loop");                                   // next byte

    emitter.label("__rt_nl2br_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}
