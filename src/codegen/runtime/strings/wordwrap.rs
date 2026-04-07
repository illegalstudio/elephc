use crate::codegen::emit::Emitter;

/// wordwrap: wrap text at word boundaries.
/// Input: x1/x2=string, x3=width, x4/x5=break_str. Output: x1/x2=result.
pub fn emit_wordwrap(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: wordwrap ---");
    emitter.label_global("__rt_wordwrap");
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set frame pointer
    emitter.instruction("stp x4, x5, [sp]");                                    // save break string ptr/len
    emitter.instruction("str x3, [sp, #16]");                                   // save width

    // -- set up concat_buf --
    emitter.adrp("x6", "_concat_off");                           // load concat offset page
    emitter.add_lo12("x6", "x6", "_concat_off");                     // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.adrp("x7", "_concat_buf");                           // load concat buffer page
    emitter.add_lo12("x7", "x7", "_concat_buf");                     // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save result start
    emitter.instruction("mov x10, #0");                                         // current line length

    emitter.label("__rt_wordwrap_loop");
    emitter.instruction("cbz x2, __rt_wordwrap_done");                          // no input left → done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining

    // -- check for existing newlines (reset counter) --
    emitter.instruction("cmp w12, #10");                                        // is it '\n'?
    emitter.instruction("b.ne __rt_wordwrap_check");                            // no → check width
    emitter.instruction("strb w12, [x9], #1");                                  // store newline
    emitter.instruction("mov x10, #0");                                         // reset line length
    emitter.instruction("b __rt_wordwrap_loop");                                // next byte

    emitter.label("__rt_wordwrap_check");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload width
    emitter.instruction("cmp x10, x3");                                         // line length >= width?
    emitter.instruction("b.lt __rt_wordwrap_store");                            // no → just store char

    // -- insert break string at width boundary --
    emitter.instruction("ldp x4, x5, [sp]");                                    // reload break string
    emitter.instruction("mov x14, #0");                                         // break copy index
    emitter.label("__rt_wordwrap_brk");
    emitter.instruction("cmp x14, x5");                                         // all break chars written?
    emitter.instruction("b.ge __rt_wordwrap_brk_done");                         // yes → continue with char
    emitter.instruction("ldrb w13, [x4, x14]");                                 // load break char
    emitter.instruction("strb w13, [x9], #1");                                  // write to output
    emitter.instruction("add x14, x14, #1");                                    // next break char
    emitter.instruction("b __rt_wordwrap_brk");                                 // continue
    emitter.label("__rt_wordwrap_brk_done");
    emitter.instruction("mov x10, #0");                                         // reset line length

    emitter.label("__rt_wordwrap_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store current byte
    emitter.instruction("add x10, x10, #1");                                    // increment line length
    emitter.instruction("b __rt_wordwrap_loop");                                // next byte

    emitter.label("__rt_wordwrap_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // result pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length
    emitter.adrp("x6", "_concat_off");                           // update concat offset
    emitter.add_lo12("x6", "x6", "_concat_off");                     // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame
    emitter.instruction("add sp, sp, #48");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}
