use crate::codegen::emit::Emitter;

/// base64_encode: standard base64 encoding (3 input bytes -> 4 output chars).
/// Input: x1/x2=string. Output: x1/x2=result in concat_buf.
/// Uses _b64_encode_tbl data section for the lookup table.
pub fn emit_base64_encode(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: base64_encode ---");
    emitter.label_global("__rt_base64_encode");

    // -- set up concat_buf destination --
    emitter.adrp("x6", "_concat_off");                           // load concat offset page
    emitter.add_lo12("x6", "x6", "_concat_off");                     // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.adrp("x7", "_concat_buf");                           // load concat buffer page
    emitter.add_lo12("x7", "x7", "_concat_buf");                     // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    // -- load base64 lookup table --
    emitter.adrp("x15", "_b64_encode_tbl");                      // load b64 table page
    emitter.add_lo12("x15", "x15", "_b64_encode_tbl");               // resolve table address

    // -- process 3 bytes at a time --
    emitter.label("__rt_b64enc_loop");
    emitter.instruction("cmp x11, #3");                                         // at least 3 bytes left?
    emitter.instruction("b.lt __rt_b64enc_remainder");                          // no -> handle remainder

    // -- load 3 source bytes --
    emitter.instruction("ldrb w12, [x1], #1");                                  // byte 0
    emitter.instruction("ldrb w13, [x1], #1");                                  // byte 1
    emitter.instruction("ldrb w14, [x1], #1");                                  // byte 2
    emitter.instruction("sub x11, x11, #3");                                    // consumed 3 bytes

    // -- encode char 0: top 6 bits of byte 0 --
    emitter.instruction("lsr w16, w12, #2");                                    // byte0 >> 2
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup table[index]
    emitter.instruction("strb w16, [x9], #1");                                  // write encoded char 0

    // -- encode char 1: bottom 2 of byte0 + top 4 of byte1 --
    emitter.instruction("and w16, w12, #0x3");                                  // byte0 & 0x3
    emitter.instruction("lsl w16, w16, #4");                                    // shift left 4
    emitter.instruction("lsr w17, w13, #4");                                    // byte1 >> 4
    emitter.instruction("orr w16, w16, w17");                                   // combine
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup table[index]
    emitter.instruction("strb w16, [x9], #1");                                  // write encoded char 1

    // -- encode char 2: bottom 4 of byte1 + top 2 of byte2 --
    emitter.instruction("and w16, w13, #0xf");                                  // byte1 & 0xf
    emitter.instruction("lsl w16, w16, #2");                                    // shift left 2
    emitter.instruction("lsr w17, w14, #6");                                    // byte2 >> 6
    emitter.instruction("orr w16, w16, w17");                                   // combine
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup table[index]
    emitter.instruction("strb w16, [x9], #1");                                  // write encoded char 2

    // -- encode char 3: bottom 6 of byte2 --
    emitter.instruction("and w16, w14, #0x3f");                                 // byte2 & 0x3f
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup table[index]
    emitter.instruction("strb w16, [x9], #1");                                  // write encoded char 3

    emitter.instruction("b __rt_b64enc_loop");                                  // next 3 bytes

    // -- handle remainder (0, 1, or 2 bytes left) --
    emitter.label("__rt_b64enc_remainder");
    emitter.instruction("cbz x11, __rt_b64enc_done");                           // 0 bytes left -> done

    emitter.instruction("cmp x11, #1");                                         // exactly 1 byte left?
    emitter.instruction("b.ne __rt_b64enc_rem2");                               // no -> 2 bytes

    // -- 1 byte remainder: 2 encoded chars + 2 padding --
    emitter.instruction("ldrb w12, [x1]");                                      // load last byte
    // char 0: top 6 bits
    emitter.instruction("lsr w16, w12, #2");                                    // byte0 >> 2
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup
    emitter.instruction("strb w16, [x9], #1");                                  // write char 0
    // char 1: bottom 2 bits << 4
    emitter.instruction("and w16, w12, #0x3");                                  // byte0 & 0x3
    emitter.instruction("lsl w16, w16, #4");                                    // shift left 4
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup
    emitter.instruction("strb w16, [x9], #1");                                  // write char 1
    // padding
    emitter.instruction("mov w16, #61");                                        // '=' padding char
    emitter.instruction("strb w16, [x9], #1");                                  // write '='
    emitter.instruction("strb w16, [x9], #1");                                  // write '='
    emitter.instruction("b __rt_b64enc_done");                                  // done

    // -- 2 byte remainder: 3 encoded chars + 1 padding --
    emitter.label("__rt_b64enc_rem2");
    emitter.instruction("ldrb w12, [x1]");                                      // load byte 0
    emitter.instruction("ldrb w13, [x1, #1]");                                  // load byte 1
    // char 0: top 6 bits of byte0
    emitter.instruction("lsr w16, w12, #2");                                    // byte0 >> 2
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup
    emitter.instruction("strb w16, [x9], #1");                                  // write char 0
    // char 1: bottom 2 of byte0 + top 4 of byte1
    emitter.instruction("and w16, w12, #0x3");                                  // byte0 & 0x3
    emitter.instruction("lsl w16, w16, #4");                                    // shift left 4
    emitter.instruction("lsr w17, w13, #4");                                    // byte1 >> 4
    emitter.instruction("orr w16, w16, w17");                                   // combine
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup
    emitter.instruction("strb w16, [x9], #1");                                  // write char 1
    // char 2: bottom 4 of byte1 << 2
    emitter.instruction("and w16, w13, #0xf");                                  // byte1 & 0xf
    emitter.instruction("lsl w16, w16, #2");                                    // shift left 2
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup
    emitter.instruction("strb w16, [x9], #1");                                  // write char 2
    // padding
    emitter.instruction("mov w16, #61");                                        // '=' padding char
    emitter.instruction("strb w16, [x9], #1");                                  // write '='

    emitter.label("__rt_b64enc_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}
