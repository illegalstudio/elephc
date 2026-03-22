use crate::codegen::emit::Emitter;

/// ltrim: strip whitespace from left. Adjusts x1 and x2.
pub fn emit_ltrim(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ltrim ---");
    emitter.label("__rt_ltrim");
    emitter.label("__rt_ltrim_loop");
    emitter.instruction("cbz x2, __rt_ltrim_done");                             // if string is empty, nothing to trim
    emitter.instruction("ldrb w9, [x1]");                                       // peek at first byte without advancing
    emitter.instruction("cmp w9, #32");                                         // check for space (0x20)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if space, skip it
    emitter.instruction("cmp w9, #9");                                          // check for tab (0x09)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if tab, skip it
    emitter.instruction("cmp w9, #10");                                         // check for newline (0x0A)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if newline, skip it
    emitter.instruction("cmp w9, #13");                                         // check for carriage return (0x0D)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if CR, skip it
    emitter.instruction("b __rt_ltrim_done");                                   // non-whitespace found, stop trimming

    // -- advance past whitespace character --
    emitter.label("__rt_ltrim_skip");
    emitter.instruction("add x1, x1, #1");                                      // advance string pointer past whitespace
    emitter.instruction("sub x2, x2, #1");                                      // decrement string length
    emitter.instruction("b __rt_ltrim_loop");                                   // check next character

    emitter.label("__rt_ltrim_done");
    emitter.instruction("ret");                                                 // return with adjusted x1 and x2
}
