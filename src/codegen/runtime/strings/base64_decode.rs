use crate::codegen::emit::Emitter;

/// base64_decode: standard base64 decoding (4 input chars -> 3 output bytes).
/// Input: x1/x2=string. Output: x1/x2=result in concat_buf.
/// Uses _b64_decode_tbl data section for the reverse lookup table.
pub fn emit_base64_decode(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: base64_decode ---");
    emitter.label_global("__rt_base64_decode");

    // -- set up concat_buf destination --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    // -- load base64 decode lookup table --
    emitter.instruction("adrp x15, _b64_decode_tbl@PAGE");                      // load b64 decode table page
    emitter.instruction("add x15, x15, _b64_decode_tbl@PAGEOFF");               // resolve table address

    // -- process 4 chars at a time --
    emitter.label("__rt_b64dec_loop");
    emitter.instruction("cmp x11, #4");                                         // at least 4 chars left?
    emitter.instruction("b.lt __rt_b64dec_done");                               // no -> done

    // -- load and decode 4 base64 chars --
    emitter.instruction("ldrb w12, [x1], #1");                                  // load char 0
    emitter.instruction("ldrb w12, [x15, x12]");                                // decode char 0 via table
    emitter.instruction("ldrb w13, [x1], #1");                                  // load char 1
    emitter.instruction("ldrb w13, [x15, x13]");                                // decode char 1 via table
    emitter.instruction("ldrb w14, [x1], #1");                                  // load char 2
    emitter.instruction("ldrb w16, [x1], #1");                                  // load char 3
    emitter.instruction("sub x11, x11, #4");                                    // consumed 4 chars

    // -- check for '=' padding in char 2 --
    emitter.instruction("cmp w14, #61");                                        // is char 2 '='?
    emitter.instruction("b.eq __rt_b64dec_pad2");                               // yes -> only 1 output byte

    // -- decode char 2 via table --
    emitter.instruction("ldrb w14, [x15, x14]");                                // decode char 2

    // -- check for '=' padding in char 3 --
    emitter.instruction("cmp w16, #61");                                        // is char 3 '='?
    emitter.instruction("b.eq __rt_b64dec_pad1");                               // yes -> only 2 output bytes

    // -- decode char 3 via table --
    emitter.instruction("ldrb w16, [x15, x16]");                                // decode char 3

    // -- output byte 0: (val0 << 2) | (val1 >> 4) --
    emitter.instruction("lsl w17, w12, #2");                                    // val0 << 2
    emitter.instruction("lsr w18, w13, #4");                                    // val1 >> 4
    emitter.instruction("orr w17, w17, w18");                                   // combine
    emitter.instruction("strb w17, [x9], #1");                                  // write byte 0

    // -- output byte 1: (val1 << 4) | (val2 >> 2) --
    emitter.instruction("and w17, w13, #0xf");                                  // val1 & 0xf
    emitter.instruction("lsl w17, w17, #4");                                    // shift left 4
    emitter.instruction("lsr w18, w14, #2");                                    // val2 >> 2
    emitter.instruction("orr w17, w17, w18");                                   // combine
    emitter.instruction("strb w17, [x9], #1");                                  // write byte 1

    // -- output byte 2: (val2 << 6) | val3 --
    emitter.instruction("and w17, w14, #0x3");                                  // val2 & 0x3
    emitter.instruction("lsl w17, w17, #6");                                    // shift left 6
    emitter.instruction("orr w17, w17, w16");                                   // combine with val3
    emitter.instruction("strb w17, [x9], #1");                                  // write byte 2
    emitter.instruction("b __rt_b64dec_loop");                                  // next 4 chars

    // -- padding: char2 is '=', only 1 output byte --
    emitter.label("__rt_b64dec_pad2");
    emitter.instruction("lsl w17, w12, #2");                                    // val0 << 2
    emitter.instruction("lsr w18, w13, #4");                                    // val1 >> 4
    emitter.instruction("orr w17, w17, w18");                                   // combine
    emitter.instruction("strb w17, [x9], #1");                                  // write byte 0
    emitter.instruction("b __rt_b64dec_done");                                  // done (skip rest)

    // -- padding: char3 is '=', only 2 output bytes --
    emitter.label("__rt_b64dec_pad1");
    // output byte 0
    emitter.instruction("lsl w17, w12, #2");                                    // val0 << 2
    emitter.instruction("lsr w18, w13, #4");                                    // val1 >> 4
    emitter.instruction("orr w17, w17, w18");                                   // combine
    emitter.instruction("strb w17, [x9], #1");                                  // write byte 0
    // output byte 1
    emitter.instruction("and w17, w13, #0xf");                                  // val1 & 0xf
    emitter.instruction("lsl w17, w17, #4");                                    // shift left 4
    emitter.instruction("lsr w18, w14, #2");                                    // val2 >> 2
    emitter.instruction("orr w17, w17, w18");                                   // combine
    emitter.instruction("strb w17, [x9], #1");                                  // write byte 1
    emitter.instruction("b __rt_b64dec_done");                                  // done (skip rest)

    emitter.label("__rt_b64dec_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}
