use crate::codegen::emit::Emitter;

/// rtrim: strip whitespace from right. Adjusts x2.
pub fn emit_rtrim(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rtrim ---");
    emitter.label("__rt_rtrim");
    emitter.label("__rt_rtrim_loop");
    emitter.instruction("cbz x2, __rt_rtrim_done");                             // if string is empty, nothing to trim
    emitter.instruction("sub x9, x2, #1");                                      // compute index of last character
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load last byte of string
    emitter.instruction("cmp w10, #32");                                        // check for space (0x20)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if space, strip it
    emitter.instruction("cmp w10, #9");                                         // check for tab (0x09)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if tab, strip it
    emitter.instruction("cmp w10, #10");                                        // check for newline (0x0A)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if newline, strip it
    emitter.instruction("cmp w10, #13");                                        // check for carriage return (0x0D)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if CR, strip it
    emitter.instruction("b __rt_rtrim_done");                                   // non-whitespace found, stop trimming

    // -- shrink length to strip trailing whitespace --
    emitter.label("__rt_rtrim_strip");
    emitter.instruction("sub x2, x2, #1");                                      // reduce length by 1 (removes last char)
    emitter.instruction("b __rt_rtrim_loop");                                   // check new last character

    emitter.label("__rt_rtrim_done");
    emitter.instruction("ret");                                                 // return with adjusted x2
}
