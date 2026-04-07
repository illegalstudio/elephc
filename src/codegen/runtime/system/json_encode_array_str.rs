use crate::codegen::emit::Emitter;

/// __rt_json_encode_array_str: encode a string array as JSON '["a","b"]'.
/// Input:  x0 = array pointer (header: cap[8], len[8], then pairs of ptr[8]+len[8])
/// Output: x1 = result ptr (in concat_buf), x2 = result len
pub(crate) fn emit_json_encode_array_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_str ---");
    emitter.label_global("__rt_json_encode_array_str");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set new frame pointer

    // -- save array pointer --
    emitter.instruction("str x0, [sp, #0]");                                    // save array ptr

    // -- get output position in concat_buf --
    emitter.adrp("x9", "_concat_off");                           // load page of concat offset
    emitter.add_lo12("x9", "x9", "_concat_off");                     // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.adrp("x11", "_concat_buf");                          // load page of concat buffer
    emitter.add_lo12("x11", "x11", "_concat_buf");                   // resolve address
    emitter.instruction("add x11, x11, x10");                                   // output position
    emitter.instruction("str x11, [sp, #8]");                                   // save output start
    emitter.instruction("str x11, [sp, #16]");                                  // save output write pos

    // -- write opening bracket --
    emitter.instruction("mov w12, #91");                                        // ASCII '['
    emitter.instruction("strb w12, [x11]");                                     // write '['
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- get array length --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array ptr
    emitter.instruction("ldr x3, [x0]");                                        // load array length from header offset 0
    emitter.instruction("str x3, [sp, #24]");                                   // save length
    emitter.instruction("str xzr, [sp, #32]");                                  // index = 0

    // -- loop through string elements --
    emitter.label("__rt_json_arr_str_loop");
    emitter.instruction("ldr x4, [sp, #32]");                                   // load index
    emitter.instruction("ldr x3, [sp, #24]");                                   // load length
    emitter.instruction("cmp x4, x3");                                          // check if done
    emitter.instruction("b.ge __rt_json_arr_str_close");                        // done

    // -- add comma separator if not first element --
    emitter.instruction("cbz x4, __rt_json_arr_str_elem");                      // skip comma for first
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #44");                                        // ASCII ','
    emitter.instruction("strb w12, [x11]");                                     // write ','
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- load string element (ptr, len pair) --
    emitter.label("__rt_json_arr_str_elem");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array ptr
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload index
    emitter.instruction("add x5, x4, x4");                                      // x5 = index * 2
    emitter.instruction("add x5, x5, #3");                                      // skip 24-byte header (3 slots)
    emitter.instruction("ldr x1, [x0, x5, lsl #3]");                            // load string ptr
    emitter.instruction("add x5, x5, #1");                                      // next slot
    emitter.instruction("ldr x2, [x0, x5, lsl #3]");                            // load string len

    // -- write opening quote --
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // write '"'
    emitter.instruction("add x11, x11, #1");                                    // advance

    // -- copy string bytes with minimal escaping --
    emitter.instruction("mov x10, #0");                                         // copy index
    emitter.label("__rt_json_arr_str_copy");
    emitter.instruction("cmp x10, x2");                                         // check if all bytes copied
    emitter.instruction("b.ge __rt_json_arr_str_quote_close");                  // done

    emitter.instruction("ldrb w12, [x1, x10]");                                 // load source byte

    // -- check for characters needing escape --
    emitter.instruction("cmp w12, #34");                                        // check for double quote
    emitter.instruction("b.eq __rt_json_arr_str_esc2");                         // escape it
    emitter.instruction("cmp w12, #92");                                        // check for backslash
    emitter.instruction("b.eq __rt_json_arr_str_esc2");                         // escape it
    emitter.instruction("cmp w12, #10");                                        // check for newline
    emitter.instruction("b.eq __rt_json_arr_str_esc_n");                        // escape it

    // -- regular char --
    emitter.instruction("strb w12, [x11]");                                     // write char
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("add x10, x10, #1");                                    // next source char
    emitter.instruction("b __rt_json_arr_str_copy");                            // continue

    // -- escape: write backslash + char --
    emitter.label("__rt_json_arr_str_esc2");
    emitter.instruction("mov w13, #92");                                        // backslash
    emitter.instruction("strb w13, [x11]");                                     // write backslash
    emitter.instruction("strb w12, [x11, #1]");                                 // write the char itself
    emitter.instruction("add x11, x11, #2");                                    // advance by 2
    emitter.instruction("add x10, x10, #1");                                    // next source char
    emitter.instruction("b __rt_json_arr_str_copy");                            // continue

    emitter.label("__rt_json_arr_str_esc_n");
    emitter.instruction("mov w13, #92");                                        // backslash
    emitter.instruction("strb w13, [x11]");                                     // write backslash
    emitter.instruction("mov w13, #110");                                       // 'n'
    emitter.instruction("strb w13, [x11, #1]");                                 // write 'n'
    emitter.instruction("add x11, x11, #2");                                    // advance by 2
    emitter.instruction("add x10, x10, #1");                                    // next source char
    emitter.instruction("b __rt_json_arr_str_copy");                            // continue

    // -- write closing quote --
    emitter.label("__rt_json_arr_str_quote_close");
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // write '"'
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- advance to next element --
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload index
    emitter.instruction("add x4, x4, #1");                                      // increment
    emitter.instruction("str x4, [sp, #32]");                                   // save index
    emitter.instruction("b __rt_json_arr_str_loop");                            // continue loop

    // -- write closing bracket --
    emitter.label("__rt_json_arr_str_close");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #93");                                        // ASCII ']'
    emitter.instruction("strb w12, [x11]");                                     // write ']'
    emitter.instruction("add x11, x11, #1");                                    // advance

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = output start
    emitter.instruction("sub x2, x11, x1");                                     // x2 = total length

    // -- update concat_off --
    emitter.adrp("x9", "_concat_off");                           // load page of concat offset
    emitter.add_lo12("x9", "x9", "_concat_off");                     // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x2");                                    // add result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- tear down and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
