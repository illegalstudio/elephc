use crate::codegen::emit::Emitter;

/// __rt_json_decode: decode a JSON string value.
/// Input:  x1=json string ptr, x2=json string len
/// Output: x1=decoded string ptr, x2=decoded string len
///
/// Supported JSON inputs:
///   - Quoted strings: "hello" → hello (with unescape)
///   - Numbers: 42 → "42" (returned as string representation)
///   - true/false/null → returned as literal string
///
/// This is a simplified implementation that handles the most common case:
/// stripping quotes and unescaping a JSON string value.
pub fn emit_json_decode(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode ---");
    emitter.label_global("__rt_json_decode");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set new frame pointer

    // -- check if input starts with a double quote --
    emitter.instruction("cbz x2, __rt_json_decode_empty");                      // empty string → return empty
    emitter.instruction("ldrb w9, [x1]");                                       // load first byte
    emitter.instruction("cmp w9, #34");                                         // check for double quote
    emitter.instruction("b.ne __rt_json_decode_passthrough");                   // not a quoted string, return as-is

    // -- it's a JSON string: strip quotes and unescape --
    emitter.instruction("str x1, [sp, #0]");                                    // save source ptr
    emitter.instruction("str x2, [sp, #8]");                                    // save source len

    // -- get output position in concat_buf --
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load page of concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                          // load page of concat buffer
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                   // resolve address
    emitter.instruction("add x11, x11, x10");                                   // output position
    emitter.instruction("str x11, [sp, #16]");                                  // save output start
    emitter.instruction("str x11, [sp, #24]");                                  // save output write pos

    // -- skip opening quote, process until closing quote --
    emitter.instruction("mov x12, #1");                                         // source index (skip opening quote)
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload source len
    emitter.instruction("sub x2, x2, #1");                                      // subtract 1 for closing quote

    emitter.label("__rt_json_decode_loop");
    emitter.instruction("cmp x12, x2");                                         // check if at closing quote
    emitter.instruction("b.ge __rt_json_decode_done");                          // done

    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source ptr
    emitter.instruction("ldrb w9, [x1, x12]");                                  // load source byte

    // -- check for escape sequence --
    emitter.instruction("cmp w9, #92");                                         // check for backslash
    emitter.instruction("b.ne __rt_json_decode_literal");                       // not escape, copy literal

    // -- process escape sequence --
    emitter.instruction("add x12, x12, #1");                                    // skip backslash
    emitter.instruction("ldrb w9, [x1, x12]");                                  // load escaped char

    emitter.instruction("cmp w9, #110");                                        // check for 'n' (newline)
    emitter.instruction("b.ne __rt_json_decode_esc_not_n");                     // not newline
    emitter.instruction("mov w9, #10");                                         // replace with actual newline
    emitter.instruction("b __rt_json_decode_literal");                          // store it
    emitter.label("__rt_json_decode_esc_not_n");

    emitter.instruction("cmp w9, #116");                                        // check for 't' (tab)
    emitter.instruction("b.ne __rt_json_decode_esc_not_t");                     // not tab
    emitter.instruction("mov w9, #9");                                          // replace with actual tab
    emitter.instruction("b __rt_json_decode_literal");                          // store it
    emitter.label("__rt_json_decode_esc_not_t");

    emitter.instruction("cmp w9, #114");                                        // check for 'r' (carriage return)
    emitter.instruction("b.ne __rt_json_decode_literal");                       // not CR, use char as-is (handles \" and \\)
    emitter.instruction("mov w9, #13");                                         // replace with actual CR

    // -- write literal or unescaped character --
    emitter.label("__rt_json_decode_literal");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload write pos
    emitter.instruction("strb w9, [x11]");                                      // write byte
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x12, x12, #1");                                    // advance source index
    emitter.instruction("b __rt_json_decode_loop");                             // continue

    // -- finalize --
    emitter.label("__rt_json_decode_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // x1 = output start
    emitter.instruction("ldr x11, [sp, #24]");                                  // load write end
    emitter.instruction("sub x2, x11, x1");                                     // x2 = output length

    // -- update concat_off --
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load page of concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x2");                                    // add result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    emitter.instruction("b __rt_json_decode_ret");                              // return

    // -- empty input --
    emitter.label("__rt_json_decode_empty");
    emitter.instruction("mov x1, #0");                                          // null ptr
    emitter.instruction("mov x2, #0");                                          // zero length
    emitter.instruction("b __rt_json_decode_ret");                              // return

    // -- passthrough (not a quoted string) --
    emitter.label("__rt_json_decode_passthrough");
    // x1 and x2 already contain the input — return as-is

    // -- tear down and return --
    emitter.label("__rt_json_decode_ret");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
