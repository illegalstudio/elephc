use crate::codegen::emit::Emitter;

/// htmlspecialchars: replace &, ", ', <, > with HTML entities.
/// Input: x1/x2=string. Output: x1/x2=result in concat_buf.
pub fn emit_htmlspecialchars(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: htmlspecialchars ---");
    emitter.label("__rt_htmlspecialchars");

    // -- set up concat_buf destination --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_htmlsc_loop");
    emitter.instruction("cbz x11, __rt_htmlsc_done");                           // no bytes left -> done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load source byte, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining

    // -- check & (38) -> &amp; --
    emitter.instruction("cmp w12, #38");                                        // is it '&'?
    emitter.instruction("b.eq __rt_htmlsc_amp");                                // yes -> write &amp;

    // -- check " (34) -> &quot; --
    emitter.instruction("cmp w12, #34");                                        // is it '"'?
    emitter.instruction("b.eq __rt_htmlsc_quot");                               // yes -> write &quot;

    // -- check ' (39) -> &#039; --
    emitter.instruction("cmp w12, #39");                                        // is it '\''?
    emitter.instruction("b.eq __rt_htmlsc_apos");                               // yes -> write &#039;

    // -- check < (60) -> &lt; --
    emitter.instruction("cmp w12, #60");                                        // is it '<'?
    emitter.instruction("b.eq __rt_htmlsc_lt");                                 // yes -> write &lt;

    // -- check > (62) -> &gt; --
    emitter.instruction("cmp w12, #62");                                        // is it '>'?
    emitter.instruction("b.eq __rt_htmlsc_gt");                                 // yes -> write &gt;

    // -- store unmodified byte --
    emitter.instruction("strb w12, [x9], #1");                                  // store byte as-is
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &amp; (5 bytes: &, a, m, p, ;) --
    emitter.label("__rt_htmlsc_amp");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #97");                                        // 'a'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'a'
    emitter.instruction("mov w13, #109");                                       // 'm'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'm'
    emitter.instruction("mov w13, #112");                                       // 'p'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'p'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &quot; (6 bytes: &, q, u, o, t, ;) --
    emitter.label("__rt_htmlsc_quot");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #113");                                       // 'q'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'q'
    emitter.instruction("mov w13, #117");                                       // 'u'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'u'
    emitter.instruction("mov w13, #111");                                       // 'o'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'o'
    emitter.instruction("mov w13, #116");                                       // 't'
    emitter.instruction("strb w13, [x9], #1");                                  // write 't'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &#039; (6 bytes: &, #, 0, 3, 9, ;) --
    emitter.label("__rt_htmlsc_apos");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #35");                                        // '#'
    emitter.instruction("strb w13, [x9], #1");                                  // write '#'
    emitter.instruction("mov w13, #48");                                        // '0'
    emitter.instruction("strb w13, [x9], #1");                                  // write '0'
    emitter.instruction("mov w13, #51");                                        // '3'
    emitter.instruction("strb w13, [x9], #1");                                  // write '3'
    emitter.instruction("mov w13, #57");                                        // '9'
    emitter.instruction("strb w13, [x9], #1");                                  // write '9'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &lt; (4 bytes: &, l, t, ;) --
    emitter.label("__rt_htmlsc_lt");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #108");                                       // 'l'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'l'
    emitter.instruction("mov w13, #116");                                       // 't'
    emitter.instruction("strb w13, [x9], #1");                                  // write 't'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &gt; (4 bytes: &, g, t, ;) --
    emitter.label("__rt_htmlsc_gt");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #103");                                       // 'g'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'g'
    emitter.instruction("mov w13, #116");                                       // 't'
    emitter.instruction("strb w13, [x9], #1");                                  // write 't'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    emitter.label("__rt_htmlsc_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}
