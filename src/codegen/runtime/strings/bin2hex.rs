use crate::codegen::emit::Emitter;

/// bin2hex: convert binary string to hex representation.
/// Input: x1/x2=string. Output: x1/x2=result (2x length).
pub fn emit_bin2hex(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: bin2hex ---");
    emitter.label_global("__rt_bin2hex");

    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining count

    emitter.label("__rt_bin2hex_loop");
    emitter.instruction("cbz x11, __rt_bin2hex_done");                          // done if no bytes left
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    // -- high nibble --
    emitter.instruction("lsr w13, w12, #4");                                    // extract high 4 bits
    emitter.instruction("cmp w13, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_bin2hex_hi_af");                             // yes → use a-f
    emitter.instruction("add w13, w13, #48");                                   // convert 0-9 to '0'-'9'
    emitter.instruction("b __rt_bin2hex_hi_store");                             // store
    emitter.label("__rt_bin2hex_hi_af");
    emitter.instruction("add w13, w13, #87");                                   // convert 10-15 to 'a'-'f'
    emitter.label("__rt_bin2hex_hi_store");
    emitter.instruction("strb w13, [x9], #1");                                  // write high nibble hex char
    // -- low nibble --
    emitter.instruction("and w13, w12, #0xf");                                  // extract low 4 bits
    emitter.instruction("cmp w13, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_bin2hex_lo_af");                             // yes → use a-f
    emitter.instruction("add w13, w13, #48");                                   // convert 0-9 to '0'-'9'
    emitter.instruction("b __rt_bin2hex_lo_store");                             // store
    emitter.label("__rt_bin2hex_lo_af");
    emitter.instruction("add w13, w13, #87");                                   // convert 10-15 to 'a'-'f'
    emitter.label("__rt_bin2hex_lo_store");
    emitter.instruction("strb w13, [x9], #1");                                  // write low nibble hex char
    emitter.instruction("b __rt_bin2hex_loop");                                 // next byte

    emitter.label("__rt_bin2hex_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}
