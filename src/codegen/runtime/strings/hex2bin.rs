use crate::codegen::emit::Emitter;

/// hex2bin: convert hex string to binary.
/// Input: x1/x2=hex_string. Output: x1/x2=result (half length).
pub fn emit_hex2bin(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hex2bin ---");
    emitter.label_global("__rt_hex2bin");

    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining hex chars

    emitter.label("__rt_hex2bin_loop");
    emitter.instruction("cmp x11, #2");                                         // need at least 2 hex chars
    emitter.instruction("b.lt __rt_hex2bin_done");                              // not enough → done

    // -- parse high nibble (inline hex digit conversion) --
    emitter.instruction("ldrb w12, [x1], #1");                                  // load first hex char
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le 1f");                                             // yes → numeric
    emitter.instruction("cmp w12, #70");                                        // <= 'F'?
    emitter.instruction("b.le 2f");                                             // yes → uppercase
    emitter.instruction("sub w12, w12, #87");                                   // 'a'-'f' → 10-15
    emitter.instruction("b 3f");                                                // done with high nibble
    emitter.raw("1:");
    emitter.instruction("sub w12, w12, #48");                                   // '0'-'9' → 0-9
    emitter.instruction("b 3f");                                                // done
    emitter.raw("2:");
    emitter.instruction("sub w12, w12, #55");                                   // 'A'-'F' → 10-15
    emitter.raw("3:");
    emitter.instruction("lsl w13, w12, #4");                                    // shift to high nibble

    // -- parse low nibble (inline hex digit conversion) --
    emitter.instruction("ldrb w12, [x1], #1");                                  // load second hex char
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le 4f");                                             // yes → numeric
    emitter.instruction("cmp w12, #70");                                        // <= 'F'?
    emitter.instruction("b.le 5f");                                             // yes → uppercase
    emitter.instruction("sub w12, w12, #87");                                   // 'a'-'f' → 10-15
    emitter.instruction("b 6f");                                                // done with low nibble
    emitter.raw("4:");
    emitter.instruction("sub w12, w12, #48");                                   // '0'-'9' → 0-9
    emitter.instruction("b 6f");                                                // done
    emitter.raw("5:");
    emitter.instruction("sub w12, w12, #55");                                   // 'A'-'F' → 10-15
    emitter.raw("6:");
    emitter.instruction("orr w13, w13, w12");                                   // combine high and low nibbles
    emitter.instruction("strb w13, [x9], #1");                                  // store decoded byte
    emitter.instruction("sub x11, x11, #2");                                    // consumed 2 hex chars
    emitter.instruction("b __rt_hex2bin_loop");                                 // next pair

    emitter.label("__rt_hex2bin_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store
    emitter.instruction("ret");                                                 // return
}
