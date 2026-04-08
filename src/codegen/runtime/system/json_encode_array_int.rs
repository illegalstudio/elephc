use crate::codegen::emit::Emitter;

/// __rt_json_encode_array_int: encode an int array as JSON "[1,2,3]".
/// Input:  x0 = array pointer (header: capacity[8], length[8], then elements[8 each])
/// Output: x1 = result ptr (in concat_buf), x2 = result len
pub(crate) fn emit_json_encode_array_int(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_int ---");
    emitter.label_global("__rt_json_encode_array_int");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set new frame pointer

    // -- save array pointer --
    emitter.instruction("str x0, [sp, #0]");                                    // save array ptr

    // -- get output position in concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
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

    // -- loop through elements --
    emitter.label("__rt_json_arr_int_loop");
    emitter.instruction("ldr x4, [sp, #32]");                                   // load index
    emitter.instruction("ldr x3, [sp, #24]");                                   // load length
    emitter.instruction("cmp x4, x3");                                          // check if done
    emitter.instruction("b.ge __rt_json_arr_int_close");                        // done

    // -- add comma separator if not first element --
    emitter.instruction("cbz x4, __rt_json_arr_int_elem");                      // skip comma for first element
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #44");                                        // ASCII ','
    emitter.instruction("strb w12, [x11]");                                     // write ','
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- load element value and convert to string --
    emitter.label("__rt_json_arr_int_elem");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array ptr
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload index
    emitter.instruction("add x4, x4, #3");                                      // skip 24-byte header (3 * 8 bytes)
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load element value
    emitter.instruction("bl __rt_itoa");                                        // convert to string → x1/x2

    // -- copy itoa result to output --
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov x10, #0");                                         // copy index
    emitter.label("__rt_json_arr_int_copy");
    emitter.instruction("cmp x10, x2");                                         // check if all bytes copied
    emitter.instruction("b.ge __rt_json_arr_int_next");                         // done copying
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load byte from itoa
    emitter.instruction("strb w12, [x11, x10]");                                // store to output
    emitter.instruction("add x10, x10, #1");                                    // increment
    emitter.instruction("b __rt_json_arr_int_copy");                            // continue

    emitter.label("__rt_json_arr_int_next");
    emitter.instruction("add x11, x11, x2");                                    // advance write pos
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload index
    emitter.instruction("add x4, x4, #1");                                      // increment
    emitter.instruction("str x4, [sp, #32]");                                   // save index
    emitter.instruction("b __rt_json_arr_int_loop");                            // continue loop

    // -- write closing bracket --
    emitter.label("__rt_json_arr_int_close");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #93");                                        // ASCII ']'
    emitter.instruction("strb w12, [x11]");                                     // write ']'
    emitter.instruction("add x11, x11, #1");                                    // advance

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = output start
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
