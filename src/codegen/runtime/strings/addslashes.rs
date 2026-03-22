use crate::codegen::emit::Emitter;

/// addslashes: escape single quotes, double quotes, backslashes with backslash.
/// Input: x1/x2=string. Output: x1/x2=result.
pub fn emit_addslashes(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: addslashes ---");
    emitter.label("__rt_addslashes");

    // -- set up concat_buf destination --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_addslashes_loop");
    emitter.instruction("cbz x11, __rt_addslashes_done");                       // no bytes left → done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load source byte, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    // -- check if char needs escaping --
    emitter.instruction("cmp w12, #39");                                        // single quote?
    emitter.instruction("b.eq __rt_addslashes_esc");                            // yes → escape it
    emitter.instruction("cmp w12, #34");                                        // double quote?
    emitter.instruction("b.eq __rt_addslashes_esc");                            // yes → escape it
    emitter.instruction("cmp w12, #92");                                        // backslash?
    emitter.instruction("b.eq __rt_addslashes_esc");                            // yes → escape it
    // -- store unescaped byte --
    emitter.instruction("strb w12, [x9], #1");                                  // store byte as-is
    emitter.instruction("b __rt_addslashes_loop");                              // next byte

    emitter.label("__rt_addslashes_esc");
    emitter.instruction("mov w13, #92");                                        // backslash character
    emitter.instruction("strb w13, [x9], #1");                                  // write escape backslash
    emitter.instruction("strb w12, [x9], #1");                                  // write the original char
    emitter.instruction("b __rt_addslashes_loop");                              // next byte

    emitter.label("__rt_addslashes_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}
