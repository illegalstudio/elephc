use crate::codegen::emit::Emitter;

/// html_entity_decode: decode &amp;, &lt;, &gt;, &quot;, &#039; back to chars.
/// Input: x1/x2=string. Output: x1/x2=result in concat_buf.
pub fn emit_html_entity_decode(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: html_entity_decode ---");
    emitter.label_global("__rt_html_entity_decode");

    // -- set up concat_buf destination --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_hed_loop");
    emitter.instruction("cbz x11, __rt_hed_done");                              // no bytes left → done
    emitter.instruction("ldrb w12, [x1]");                                      // peek at current byte
    emitter.instruction("cmp w12, #38");                                        // is it '&'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no → copy as-is

    // -- try &lt; (4 chars: &lt;) --
    emitter.instruction("cmp x11, #4");                                         // need at least 4
    emitter.instruction("b.lt __rt_hed_copy");                                  // not enough
    emitter.instruction("ldrb w13, [x1, #1]");                                  // 2nd char
    emitter.instruction("cmp w13, #108");                                       // 'l'?
    emitter.instruction("b.ne __rt_hed_not_lt");                                // no
    emitter.instruction("ldrb w13, [x1, #2]");                                  // 3rd char
    emitter.instruction("cmp w13, #116");                                       // 't'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #3]");                                  // 4th char
    emitter.instruction("cmp w13, #59");                                        // ';'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("mov w12, #60");                                        // matched &lt; → '<'
    emitter.instruction("strb w12, [x9], #1");                                  // write '<'
    emitter.instruction("add x1, x1, #4");                                      // skip 4 source bytes
    emitter.instruction("sub x11, x11, #4");                                    // decrement remaining
    emitter.instruction("b __rt_hed_loop");                                     // next

    // -- try &gt; (4 chars: &gt;) --
    emitter.label("__rt_hed_not_lt");
    emitter.instruction("cmp w13, #103");                                       // 'g'?
    emitter.instruction("b.ne __rt_hed_not_gt");                                // no
    emitter.instruction("ldrb w13, [x1, #2]");                                  // 3rd char
    emitter.instruction("cmp w13, #116");                                       // 't'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #3]");                                  // 4th char
    emitter.instruction("cmp w13, #59");                                        // ';'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("mov w12, #62");                                        // matched &gt; → '>'
    emitter.instruction("strb w12, [x9], #1");                                  // write '>'
    emitter.instruction("add x1, x1, #4");                                      // skip 4
    emitter.instruction("sub x11, x11, #4");                                    // decrement
    emitter.instruction("b __rt_hed_loop");                                     // next

    // -- try &amp; (5 chars) --
    emitter.label("__rt_hed_not_gt");
    emitter.instruction("cmp x11, #5");                                         // need at least 5
    emitter.instruction("b.lt __rt_hed_try_long");                              // not enough for &amp;
    emitter.instruction("cmp w13, #97");                                        // 'a'?
    emitter.instruction("b.ne __rt_hed_try_long");                              // no
    emitter.instruction("ldrb w13, [x1, #2]");                                  // 3rd char
    emitter.instruction("cmp w13, #109");                                       // 'm'?
    emitter.instruction("b.ne __rt_hed_try_long");                              // no
    emitter.instruction("ldrb w13, [x1, #3]");                                  // 4th char
    emitter.instruction("cmp w13, #112");                                       // 'p'?
    emitter.instruction("b.ne __rt_hed_try_long");                              // no
    emitter.instruction("ldrb w13, [x1, #4]");                                  // 5th char
    emitter.instruction("cmp w13, #59");                                        // ';'?
    emitter.instruction("b.ne __rt_hed_try_long");                              // no
    emitter.instruction("mov w12, #38");                                        // matched &amp; → '&'
    emitter.instruction("strb w12, [x9], #1");                                  // write '&'
    emitter.instruction("add x1, x1, #5");                                      // skip 5
    emitter.instruction("sub x11, x11, #5");                                    // decrement
    emitter.instruction("b __rt_hed_loop");                                     // next

    // -- try &quot; or &#039; (6 chars) --
    emitter.label("__rt_hed_try_long");
    emitter.instruction("cmp x11, #6");                                         // need at least 6
    emitter.instruction("b.lt __rt_hed_copy");                                  // not enough
    emitter.instruction("ldrb w13, [x1, #1]");                                  // reload 2nd char
    emitter.instruction("cmp w13, #113");                                       // 'q'? (&quot;)
    emitter.instruction("b.ne __rt_hed_try_apos");                              // no

    // -- try &quot; --
    emitter.instruction("ldrb w13, [x1, #2]");                                  // 3rd
    emitter.instruction("cmp w13, #117");                                       // 'u'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #3]");                                  // 4th
    emitter.instruction("cmp w13, #111");                                       // 'o'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #4]");                                  // 5th
    emitter.instruction("cmp w13, #116");                                       // 't'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #5]");                                  // 6th
    emitter.instruction("cmp w13, #59");                                        // ';'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("mov w12, #34");                                        // matched &quot; → '"'
    emitter.instruction("strb w12, [x9], #1");                                  // write '"'
    emitter.instruction("add x1, x1, #6");                                      // skip 6
    emitter.instruction("sub x11, x11, #6");                                    // decrement
    emitter.instruction("b __rt_hed_loop");                                     // next

    // -- try &#039; --
    emitter.label("__rt_hed_try_apos");
    emitter.instruction("cmp w13, #35");                                        // '#'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #2]");                                  // 3rd
    emitter.instruction("cmp w13, #48");                                        // '0'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #3]");                                  // 4th
    emitter.instruction("cmp w13, #51");                                        // '3'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #4]");                                  // 5th
    emitter.instruction("cmp w13, #57");                                        // '9'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #5]");                                  // 6th
    emitter.instruction("cmp w13, #59");                                        // ';'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("mov w12, #39");                                        // matched &#039; → '\''
    emitter.instruction("strb w12, [x9], #1");                                  // write '\''
    emitter.instruction("add x1, x1, #6");                                      // skip 6
    emitter.instruction("sub x11, x11, #6");                                    // decrement
    emitter.instruction("b __rt_hed_loop");                                     // next

    // -- copy single byte as-is --
    emitter.label("__rt_hed_copy");
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("strb w12, [x9], #1");                                  // store to output
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.instruction("b __rt_hed_loop");                                     // next byte

    // -- finalize --
    emitter.label("__rt_hed_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}
