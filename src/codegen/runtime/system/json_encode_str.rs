use crate::codegen::emit::Emitter;

/// __rt_json_encode_str: JSON-encode a string (add quotes, escape special chars).
/// Input:  x1 = string ptr, x2 = string len
/// Output: x1 = result ptr (in concat_buf), x2 = result len
pub(crate) fn emit_json_encode_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_str ---");
    emitter.label_global("__rt_json_encode_str");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set new frame pointer

    // -- save inputs --
    emitter.instruction("str x1, [sp, #0]");                                    // save source ptr
    emitter.instruction("str x2, [sp, #8]");                                    // save source len

    // -- get output position in concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // output position
    emitter.instruction("str x11, [sp, #16]");                                  // save output start
    emitter.instruction("str x11, [sp, #24]");                                  // save output write pos

    // -- write opening quote --
    emitter.instruction("mov w12, #34");                                        // ASCII double quote
    emitter.instruction("strb w12, [x11]");                                     // write opening "
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos

    // -- loop through source string, escaping special chars --
    emitter.instruction("mov x13, #0");                                         // source index
    emitter.label("__rt_json_str_loop");
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload source len
    emitter.instruction("cmp x13, x2");                                         // check if done
    emitter.instruction("b.ge __rt_json_str_close");                            // done, write closing quote

    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source ptr
    emitter.instruction("ldrb w14, [x1, x13]");                                 // load source byte
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload write pos

    // -- check for special characters that need escaping --
    emitter.instruction("cmp w14, #34");                                        // check for double quote
    emitter.instruction("b.eq __rt_json_str_esc_quote");                        // escape it

    emitter.instruction("cmp w14, #92");                                        // check for backslash
    emitter.instruction("b.eq __rt_json_str_esc_backslash");                    // escape it

    emitter.instruction("cmp w14, #10");                                        // check for newline
    emitter.instruction("b.eq __rt_json_str_esc_n");                            // escape it

    emitter.instruction("cmp w14, #13");                                        // check for carriage return
    emitter.instruction("b.eq __rt_json_str_esc_r");                            // escape it

    emitter.instruction("cmp w14, #9");                                         // check for tab
    emitter.instruction("b.eq __rt_json_str_esc_t");                            // escape it

    // -- regular character, copy as-is --
    emitter.instruction("strb w14, [x11]");                                     // write char
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    // -- escape sequences --
    emitter.label("__rt_json_str_esc_quote");
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write backslash
    emitter.instruction("mov w15, #34");                                        // double quote
    emitter.instruction("strb w15, [x11, #1]");                                 // write escaped quote
    emitter.instruction("add x11, x11, #2");                                    // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    emitter.label("__rt_json_str_esc_backslash");
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write first backslash
    emitter.instruction("strb w15, [x11, #1]");                                 // write second backslash
    emitter.instruction("add x11, x11, #2");                                    // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    emitter.label("__rt_json_str_esc_n");
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write backslash
    emitter.instruction("mov w15, #110");                                       // 'n'
    emitter.instruction("strb w15, [x11, #1]");                                 // write 'n'
    emitter.instruction("add x11, x11, #2");                                    // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    emitter.label("__rt_json_str_esc_r");
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write backslash
    emitter.instruction("mov w15, #114");                                       // 'r'
    emitter.instruction("strb w15, [x11, #1]");                                 // write 'r'
    emitter.instruction("add x11, x11, #2");                                    // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    emitter.label("__rt_json_str_esc_t");
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write backslash
    emitter.instruction("mov w15, #116");                                       // 't'
    emitter.instruction("strb w15, [x11, #1]");                                 // write 't'
    emitter.instruction("add x11, x11, #2");                                    // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    // -- write closing quote --
    emitter.label("__rt_json_str_close");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload write pos
    emitter.instruction("mov w12, #34");                                        // ASCII double quote
    emitter.instruction("strb w12, [x11]");                                     // write closing "
    emitter.instruction("add x11, x11, #1");                                    // advance past closing quote

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #16]");                                   // x1 = output start
    emitter.instruction("sub x2, x11, x1");                                     // x2 = total length

    // -- update concat_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x2");                                    // add result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- tear down and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
