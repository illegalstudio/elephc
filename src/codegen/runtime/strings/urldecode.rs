use crate::codegen::emit::Emitter;

/// urldecode: decode %XX hex sequences and '+' to space.
/// Input: x1/x2=string. Output: x1/x2=result in concat_buf.
pub fn emit_urldecode(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: urldecode ---");
    emitter.label_global("__rt_urldecode");

    // -- set up concat_buf destination --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_urldecode_loop");
    emitter.instruction("cbz x11, __rt_urldecode_done");                        // no bytes left -> done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load source byte, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining

    // -- check '+' -> space --
    emitter.instruction("cmp w12, #43");                                        // is it '+'?
    emitter.instruction("b.ne __rt_urldecode_chk_pct");                         // no -> check '%'
    emitter.instruction("mov w13, #32");                                        // space character
    emitter.instruction("strb w13, [x9], #1");                                  // write space
    emitter.instruction("b __rt_urldecode_loop");                               // next byte

    // -- check '%' -> decode hex pair --
    emitter.label("__rt_urldecode_chk_pct");
    emitter.instruction("cmp w12, #37");                                        // is it '%'?
    emitter.instruction("b.ne __rt_urldecode_store");                           // no -> store as-is
    emitter.instruction("cmp x11, #2");                                         // need at least 2 more bytes
    emitter.instruction("b.lt __rt_urldecode_store_pct");                       // not enough -> store '%'

    // -- decode high nibble --
    emitter.instruction("ldrb w12, [x1], #1");                                  // load first hex char
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le __rt_urldecode_hi_num");                          // yes -> numeric
    emitter.instruction("cmp w12, #70");                                        // <= 'F'?
    emitter.instruction("b.le __rt_urldecode_hi_uc");                           // yes -> uppercase
    emitter.instruction("sub w12, w12, #87");                                   // 'a'-'f' -> 10-15
    emitter.instruction("b __rt_urldecode_hi_done");                            // done with high nibble
    emitter.label("__rt_urldecode_hi_num");
    emitter.instruction("sub w12, w12, #48");                                   // '0'-'9' -> 0-9
    emitter.instruction("b __rt_urldecode_hi_done");                            // done
    emitter.label("__rt_urldecode_hi_uc");
    emitter.instruction("sub w12, w12, #55");                                   // 'A'-'F' -> 10-15
    emitter.label("__rt_urldecode_hi_done");
    emitter.instruction("lsl w13, w12, #4");                                    // shift to high nibble

    // -- decode low nibble --
    emitter.instruction("ldrb w12, [x1], #1");                                  // load second hex char
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le __rt_urldecode_lo_num");                          // yes -> numeric
    emitter.instruction("cmp w12, #70");                                        // <= 'F'?
    emitter.instruction("b.le __rt_urldecode_lo_uc");                           // yes -> uppercase
    emitter.instruction("sub w12, w12, #87");                                   // 'a'-'f' -> 10-15
    emitter.instruction("b __rt_urldecode_lo_done");                            // done with low nibble
    emitter.label("__rt_urldecode_lo_num");
    emitter.instruction("sub w12, w12, #48");                                   // '0'-'9' -> 0-9
    emitter.instruction("b __rt_urldecode_lo_done");                            // done
    emitter.label("__rt_urldecode_lo_uc");
    emitter.instruction("sub w12, w12, #55");                                   // 'A'-'F' -> 10-15
    emitter.label("__rt_urldecode_lo_done");
    emitter.instruction("orr w13, w13, w12");                                   // combine high and low nibbles
    emitter.instruction("strb w13, [x9], #1");                                  // store decoded byte
    emitter.instruction("b __rt_urldecode_loop");                               // next iteration

    // -- store '%' as-is (not enough chars for hex pair) --
    emitter.label("__rt_urldecode_store_pct");
    emitter.instruction("mov w12, #37");                                        // '%' character
    emitter.label("__rt_urldecode_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store byte as-is
    emitter.instruction("b __rt_urldecode_loop");                               // next byte

    emitter.label("__rt_urldecode_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}
