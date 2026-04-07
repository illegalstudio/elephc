use crate::codegen::emit::Emitter;

/// urlencode: percent-encode non-alphanumeric chars except -_. and space->+.
/// Input: x1/x2=string. Output: x1/x2=result in concat_buf.
pub fn emit_urlencode(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: urlencode ---");
    emitter.label_global("__rt_urlencode");

    // -- set up concat_buf destination --
    emitter.adrp("x6", "_concat_off");                           // load concat offset page
    emitter.add_lo12("x6", "x6", "_concat_off");                     // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.adrp("x7", "_concat_buf");                           // load concat buffer page
    emitter.add_lo12("x7", "x7", "_concat_buf");                     // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_urlencode_loop");
    emitter.instruction("cbz x11, __rt_urlencode_done");                        // no bytes left -> done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load source byte, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining

    // -- check if space -> '+' --
    emitter.instruction("cmp w12, #32");                                        // is it space?
    emitter.instruction("b.ne __rt_urlencode_chk_alnum");                       // no -> check alphanumeric
    emitter.instruction("mov w13, #43");                                        // '+' character
    emitter.instruction("strb w13, [x9], #1");                                  // write '+'
    emitter.instruction("b __rt_urlencode_loop");                               // next byte

    // -- check alphanumeric and safe chars --
    emitter.label("__rt_urlencode_chk_alnum");
    // -- check A-Z --
    emitter.instruction("cmp w12, #65");                                        // >= 'A'?
    emitter.instruction("b.lt __rt_urlencode_chk_safe");                        // no -> check safe chars
    emitter.instruction("cmp w12, #90");                                        // <= 'Z'?
    emitter.instruction("b.le __rt_urlencode_passthru");                        // yes -> pass through
    // -- check a-z --
    emitter.instruction("cmp w12, #97");                                        // >= 'a'?
    emitter.instruction("b.lt __rt_urlencode_chk_safe");                        // no -> check safe chars
    emitter.instruction("cmp w12, #122");                                       // <= 'z'?
    emitter.instruction("b.le __rt_urlencode_passthru");                        // yes -> pass through
    // -- check 0-9 --
    emitter.instruction("cmp w12, #48");                                        // >= '0'?
    emitter.instruction("b.lt __rt_urlencode_chk_safe");                        // no -> check safe chars
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le __rt_urlencode_passthru");                        // yes -> pass through

    // -- check safe chars: - (45), _ (95), . (46) --
    emitter.label("__rt_urlencode_chk_safe");
    emitter.instruction("cmp w12, #45");                                        // is it '-'?
    emitter.instruction("b.eq __rt_urlencode_passthru");                        // yes -> pass through
    emitter.instruction("cmp w12, #95");                                        // is it '_'?
    emitter.instruction("b.eq __rt_urlencode_passthru");                        // yes -> pass through
    emitter.instruction("cmp w12, #46");                                        // is it '.'?
    emitter.instruction("b.eq __rt_urlencode_passthru");                        // yes -> pass through

    // -- percent-encode: write %XX --
    emitter.instruction("mov w13, #37");                                        // '%' character
    emitter.instruction("strb w13, [x9], #1");                                  // write '%'
    // -- high nibble --
    emitter.instruction("lsr w13, w12, #4");                                    // extract high 4 bits
    emitter.instruction("cmp w13, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_urlencode_hi_af");                           // yes -> use A-F
    emitter.instruction("add w13, w13, #48");                                   // convert 0-9 to '0'-'9'
    emitter.instruction("b __rt_urlencode_hi_st");                              // store
    emitter.label("__rt_urlencode_hi_af");
    emitter.instruction("add w13, w13, #55");                                   // convert 10-15 to 'A'-'F'
    emitter.label("__rt_urlencode_hi_st");
    emitter.instruction("strb w13, [x9], #1");                                  // write high nibble hex char
    // -- low nibble --
    emitter.instruction("and w13, w12, #0xf");                                  // extract low 4 bits
    emitter.instruction("cmp w13, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_urlencode_lo_af");                           // yes -> use A-F
    emitter.instruction("add w13, w13, #48");                                   // convert 0-9 to '0'-'9'
    emitter.instruction("b __rt_urlencode_lo_st");                              // store
    emitter.label("__rt_urlencode_lo_af");
    emitter.instruction("add w13, w13, #55");                                   // convert 10-15 to 'A'-'F'
    emitter.label("__rt_urlencode_lo_st");
    emitter.instruction("strb w13, [x9], #1");                                  // write low nibble hex char
    emitter.instruction("b __rt_urlencode_loop");                               // next byte

    // -- pass through byte unchanged --
    emitter.label("__rt_urlencode_passthru");
    emitter.instruction("strb w12, [x9], #1");                                  // store byte as-is
    emitter.instruction("b __rt_urlencode_loop");                               // next byte

    emitter.label("__rt_urlencode_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}
