use crate::codegen::emit::Emitter;

/// __rt_json_encode_bool: convert boolean to "true" or "false" JSON string.
/// Input:  x0 = bool value (0 or 1)
/// Output: x1 = string ptr, x2 = string len
pub fn emit_json_encode_bool(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_bool ---");
    emitter.label("__rt_json_encode_bool");

    emitter.instruction("cbnz x0, __rt_json_encode_true");                          // if true, emit "true"

    // -- false --
    emitter.instruction("adrp x1, _json_false@PAGE");                               // load page of "false" string
    emitter.instruction("add x1, x1, _json_false@PAGEOFF");                         // resolve "false" address
    emitter.instruction("mov x2, #5");                                              // length of "false"
    emitter.instruction("ret");                                                     // return

    // -- true --
    emitter.label("__rt_json_encode_true");
    emitter.instruction("adrp x1, _json_true@PAGE");                                // load page of "true" string
    emitter.instruction("add x1, x1, _json_true@PAGEOFF");                          // resolve "true" address
    emitter.instruction("mov x2, #4");                                              // length of "true"
    emitter.instruction("ret");                                                     // return
}

/// __rt_json_encode_null: produce the "null" JSON string.
/// Output: x1 = string ptr, x2 = string len
pub fn emit_json_encode_null(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_null ---");
    emitter.label("__rt_json_encode_null");

    emitter.instruction("adrp x1, _json_null@PAGE");                                // load page of "null" string
    emitter.instruction("add x1, x1, _json_null@PAGEOFF");                          // resolve "null" address
    emitter.instruction("mov x2, #4");                                              // length of "null"
    emitter.instruction("ret");                                                     // return
}

/// __rt_json_encode_str: JSON-encode a string (add quotes, escape special chars).
/// Input:  x1 = string ptr, x2 = string len
/// Output: x1 = result ptr (in concat_buf), x2 = result len
pub fn emit_json_encode_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_str ---");
    emitter.label("__rt_json_encode_str");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                         // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                                 // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                        // set new frame pointer

    // -- save inputs --
    emitter.instruction("str x1, [sp, #0]");                                        // save source ptr
    emitter.instruction("str x2, [sp, #8]");                                        // save source len

    // -- get output position in concat_buf --
    emitter.instruction("adrp x9, _concat_off@PAGE");                               // load page of concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                         // resolve address
    emitter.instruction("ldr x10, [x9]");                                            // load current offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                              // load page of concat buffer
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                       // resolve address
    emitter.instruction("add x11, x11, x10");                                       // output position
    emitter.instruction("str x11, [sp, #16]");                                      // save output start
    emitter.instruction("str x11, [sp, #24]");                                      // save output write pos

    // -- write opening quote --
    emitter.instruction("mov w12, #34");                                            // ASCII double quote
    emitter.instruction("strb w12, [x11]");                                         // write opening "
    emitter.instruction("add x11, x11, #1");                                        // advance
    emitter.instruction("str x11, [sp, #24]");                                      // save write pos

    // -- loop through source string, escaping special chars --
    emitter.instruction("mov x13, #0");                                             // source index
    emitter.label("__rt_json_str_loop");
    emitter.instruction("ldr x2, [sp, #8]");                                        // reload source len
    emitter.instruction("cmp x13, x2");                                             // check if done
    emitter.instruction("b.ge __rt_json_str_close");                                // done, write closing quote

    emitter.instruction("ldr x1, [sp, #0]");                                        // reload source ptr
    emitter.instruction("ldrb w14, [x1, x13]");                                     // load source byte
    emitter.instruction("ldr x11, [sp, #24]");                                      // reload write pos

    // -- check for special characters that need escaping --
    emitter.instruction("cmp w14, #34");                                            // check for double quote
    emitter.instruction("b.eq __rt_json_str_esc_quote");                            // escape it

    emitter.instruction("cmp w14, #92");                                            // check for backslash
    emitter.instruction("b.eq __rt_json_str_esc_backslash");                        // escape it

    emitter.instruction("cmp w14, #10");                                            // check for newline
    emitter.instruction("b.eq __rt_json_str_esc_n");                                // escape it

    emitter.instruction("cmp w14, #13");                                            // check for carriage return
    emitter.instruction("b.eq __rt_json_str_esc_r");                                // escape it

    emitter.instruction("cmp w14, #9");                                             // check for tab
    emitter.instruction("b.eq __rt_json_str_esc_t");                                // escape it

    // -- regular character, copy as-is --
    emitter.instruction("strb w14, [x11]");                                         // write char
    emitter.instruction("add x11, x11, #1");                                        // advance
    emitter.instruction("str x11, [sp, #24]");                                      // save write pos
    emitter.instruction("add x13, x13, #1");                                        // next source char
    emitter.instruction("b __rt_json_str_loop");                                    // continue

    // -- escape sequences --
    emitter.label("__rt_json_str_esc_quote");
    emitter.instruction("mov w15, #92");                                            // backslash
    emitter.instruction("strb w15, [x11]");                                         // write backslash
    emitter.instruction("mov w15, #34");                                            // double quote
    emitter.instruction("strb w15, [x11, #1]");                                     // write escaped quote
    emitter.instruction("add x11, x11, #2");                                        // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                      // save write pos
    emitter.instruction("add x13, x13, #1");                                        // next source char
    emitter.instruction("b __rt_json_str_loop");                                    // continue

    emitter.label("__rt_json_str_esc_backslash");
    emitter.instruction("mov w15, #92");                                            // backslash
    emitter.instruction("strb w15, [x11]");                                         // write first backslash
    emitter.instruction("strb w15, [x11, #1]");                                     // write second backslash
    emitter.instruction("add x11, x11, #2");                                        // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                      // save write pos
    emitter.instruction("add x13, x13, #1");                                        // next source char
    emitter.instruction("b __rt_json_str_loop");                                    // continue

    emitter.label("__rt_json_str_esc_n");
    emitter.instruction("mov w15, #92");                                            // backslash
    emitter.instruction("strb w15, [x11]");                                         // write backslash
    emitter.instruction("mov w15, #110");                                           // 'n'
    emitter.instruction("strb w15, [x11, #1]");                                     // write 'n'
    emitter.instruction("add x11, x11, #2");                                        // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                      // save write pos
    emitter.instruction("add x13, x13, #1");                                        // next source char
    emitter.instruction("b __rt_json_str_loop");                                    // continue

    emitter.label("__rt_json_str_esc_r");
    emitter.instruction("mov w15, #92");                                            // backslash
    emitter.instruction("strb w15, [x11]");                                         // write backslash
    emitter.instruction("mov w15, #114");                                           // 'r'
    emitter.instruction("strb w15, [x11, #1]");                                     // write 'r'
    emitter.instruction("add x11, x11, #2");                                        // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                      // save write pos
    emitter.instruction("add x13, x13, #1");                                        // next source char
    emitter.instruction("b __rt_json_str_loop");                                    // continue

    emitter.label("__rt_json_str_esc_t");
    emitter.instruction("mov w15, #92");                                            // backslash
    emitter.instruction("strb w15, [x11]");                                         // write backslash
    emitter.instruction("mov w15, #116");                                           // 't'
    emitter.instruction("strb w15, [x11, #1]");                                     // write 't'
    emitter.instruction("add x11, x11, #2");                                        // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                      // save write pos
    emitter.instruction("add x13, x13, #1");                                        // next source char
    emitter.instruction("b __rt_json_str_loop");                                    // continue

    // -- write closing quote --
    emitter.label("__rt_json_str_close");
    emitter.instruction("ldr x11, [sp, #24]");                                      // reload write pos
    emitter.instruction("mov w12, #34");                                            // ASCII double quote
    emitter.instruction("strb w12, [x11]");                                         // write closing "
    emitter.instruction("add x11, x11, #1");                                        // advance past closing quote

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #16]");                                       // x1 = output start
    emitter.instruction("sub x2, x11, x1");                                         // x2 = total length

    // -- update concat_off --
    emitter.instruction("adrp x9, _concat_off@PAGE");                               // load page of concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                         // resolve address
    emitter.instruction("ldr x10, [x9]");                                            // load current offset
    emitter.instruction("add x10, x10, x2");                                        // add result length
    emitter.instruction("str x10, [x9]");                                            // store updated offset

    // -- tear down and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                                 // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                          // deallocate stack frame
    emitter.instruction("ret");                                                     // return to caller
}

/// __rt_json_encode_array_int: encode an int array as JSON "[1,2,3]".
/// Input:  x0 = array pointer (header: capacity[8], length[8], then elements[8 each])
/// Output: x1 = result ptr (in concat_buf), x2 = result len
pub fn emit_json_encode_array_int(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_int ---");
    emitter.label("__rt_json_encode_array_int");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                         // allocate 64 bytes
    emitter.instruction("stp x29, x30, [sp, #48]");                                 // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                        // set new frame pointer

    // -- save array pointer --
    emitter.instruction("str x0, [sp, #0]");                                        // save array ptr

    // -- get output position in concat_buf --
    emitter.instruction("adrp x9, _concat_off@PAGE");                               // load page of concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                         // resolve address
    emitter.instruction("ldr x10, [x9]");                                            // load current offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                              // load page of concat buffer
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                       // resolve address
    emitter.instruction("add x11, x11, x10");                                       // output position
    emitter.instruction("str x11, [sp, #8]");                                       // save output start
    emitter.instruction("str x11, [sp, #16]");                                      // save output write pos

    // -- write opening bracket --
    emitter.instruction("mov w12, #91");                                            // ASCII '['
    emitter.instruction("strb w12, [x11]");                                         // write '['
    emitter.instruction("add x11, x11, #1");                                        // advance
    emitter.instruction("str x11, [sp, #16]");                                      // save write pos

    // -- get array length --
    emitter.instruction("ldr x0, [sp, #0]");                                        // reload array ptr
    emitter.instruction("ldr x3, [x0]");                                              // load array length from header offset 0
    emitter.instruction("str x3, [sp, #24]");                                       // save length
    emitter.instruction("str xzr, [sp, #32]");                                      // index = 0

    // -- loop through elements --
    emitter.label("__rt_json_arr_int_loop");
    emitter.instruction("ldr x4, [sp, #32]");                                       // load index
    emitter.instruction("ldr x3, [sp, #24]");                                       // load length
    emitter.instruction("cmp x4, x3");                                              // check if done
    emitter.instruction("b.ge __rt_json_arr_int_close");                            // done

    // -- add comma separator if not first element --
    emitter.instruction("cbz x4, __rt_json_arr_int_elem");                          // skip comma for first element
    emitter.instruction("ldr x11, [sp, #16]");                                      // reload write pos
    emitter.instruction("mov w12, #44");                                            // ASCII ','
    emitter.instruction("strb w12, [x11]");                                         // write ','
    emitter.instruction("add x11, x11, #1");                                        // advance
    emitter.instruction("str x11, [sp, #16]");                                      // save write pos

    // -- load element value and convert to string --
    emitter.label("__rt_json_arr_int_elem");
    emitter.instruction("ldr x0, [sp, #0]");                                        // reload array ptr
    emitter.instruction("ldr x4, [sp, #32]");                                       // reload index
    emitter.instruction("add x4, x4, #3");                                          // skip 24-byte header (3 * 8 bytes)
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                               // load element value
    emitter.instruction("bl __rt_itoa");                                            // convert to string → x1/x2

    // -- copy itoa result to output --
    emitter.instruction("ldr x11, [sp, #16]");                                      // reload write pos
    emitter.instruction("mov x10, #0");                                             // copy index
    emitter.label("__rt_json_arr_int_copy");
    emitter.instruction("cmp x10, x2");                                             // check if all bytes copied
    emitter.instruction("b.ge __rt_json_arr_int_next");                             // done copying
    emitter.instruction("ldrb w12, [x1, x10]");                                     // load byte from itoa
    emitter.instruction("strb w12, [x11, x10]");                                    // store to output
    emitter.instruction("add x10, x10, #1");                                        // increment
    emitter.instruction("b __rt_json_arr_int_copy");                                // continue

    emitter.label("__rt_json_arr_int_next");
    emitter.instruction("add x11, x11, x2");                                        // advance write pos
    emitter.instruction("str x11, [sp, #16]");                                      // save write pos
    emitter.instruction("ldr x4, [sp, #32]");                                       // reload index
    emitter.instruction("add x4, x4, #1");                                          // increment
    emitter.instruction("str x4, [sp, #32]");                                       // save index
    emitter.instruction("b __rt_json_arr_int_loop");                                // continue loop

    // -- write closing bracket --
    emitter.label("__rt_json_arr_int_close");
    emitter.instruction("ldr x11, [sp, #16]");                                      // reload write pos
    emitter.instruction("mov w12, #93");                                            // ASCII ']'
    emitter.instruction("strb w12, [x11]");                                         // write ']'
    emitter.instruction("add x11, x11, #1");                                        // advance

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #8]");                                        // x1 = output start
    emitter.instruction("sub x2, x11, x1");                                         // x2 = total length

    // -- update concat_off --
    emitter.instruction("adrp x9, _concat_off@PAGE");                               // load page of concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                         // resolve address
    emitter.instruction("ldr x10, [x9]");                                            // load current offset
    emitter.instruction("add x10, x10, x2");                                        // add result length
    emitter.instruction("str x10, [x9]");                                            // store updated offset

    // -- tear down and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                                 // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                          // deallocate stack frame
    emitter.instruction("ret");                                                     // return to caller
}

/// __rt_json_encode_array_str: encode a string array as JSON '["a","b"]'.
/// Input:  x0 = array pointer (header: cap[8], len[8], then pairs of ptr[8]+len[8])
/// Output: x1 = result ptr (in concat_buf), x2 = result len
pub fn emit_json_encode_array_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_str ---");
    emitter.label("__rt_json_encode_array_str");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                         // allocate 64 bytes
    emitter.instruction("stp x29, x30, [sp, #48]");                                 // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                        // set new frame pointer

    // -- save array pointer --
    emitter.instruction("str x0, [sp, #0]");                                        // save array ptr

    // -- get output position in concat_buf --
    emitter.instruction("adrp x9, _concat_off@PAGE");                               // load page of concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                         // resolve address
    emitter.instruction("ldr x10, [x9]");                                            // load current offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                              // load page of concat buffer
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                       // resolve address
    emitter.instruction("add x11, x11, x10");                                       // output position
    emitter.instruction("str x11, [sp, #8]");                                       // save output start
    emitter.instruction("str x11, [sp, #16]");                                      // save output write pos

    // -- write opening bracket --
    emitter.instruction("mov w12, #91");                                            // ASCII '['
    emitter.instruction("strb w12, [x11]");                                         // write '['
    emitter.instruction("add x11, x11, #1");                                        // advance
    emitter.instruction("str x11, [sp, #16]");                                      // save write pos

    // -- get array length --
    emitter.instruction("ldr x0, [sp, #0]");                                        // reload array ptr
    emitter.instruction("ldr x3, [x0]");                                              // load array length from header offset 0
    emitter.instruction("str x3, [sp, #24]");                                       // save length
    emitter.instruction("str xzr, [sp, #32]");                                      // index = 0

    // -- loop through string elements --
    emitter.label("__rt_json_arr_str_loop");
    emitter.instruction("ldr x4, [sp, #32]");                                       // load index
    emitter.instruction("ldr x3, [sp, #24]");                                       // load length
    emitter.instruction("cmp x4, x3");                                              // check if done
    emitter.instruction("b.ge __rt_json_arr_str_close");                            // done

    // -- add comma separator if not first element --
    emitter.instruction("cbz x4, __rt_json_arr_str_elem");                          // skip comma for first
    emitter.instruction("ldr x11, [sp, #16]");                                      // reload write pos
    emitter.instruction("mov w12, #44");                                            // ASCII ','
    emitter.instruction("strb w12, [x11]");                                         // write ','
    emitter.instruction("add x11, x11, #1");                                        // advance
    emitter.instruction("str x11, [sp, #16]");                                      // save write pos

    // -- load string element (ptr, len pair) --
    emitter.label("__rt_json_arr_str_elem");
    emitter.instruction("ldr x0, [sp, #0]");                                        // reload array ptr
    emitter.instruction("ldr x4, [sp, #32]");                                       // reload index
    // String arrays: header is 24 bytes (3 slots), each element is 16 bytes (2 slots: ptr + len)
    emitter.instruction("add x5, x4, x4");                                          // x5 = index * 2
    emitter.instruction("add x5, x5, #3");                                          // skip 24-byte header (3 slots)
    emitter.instruction("ldr x1, [x0, x5, lsl #3]");                               // load string ptr
    emitter.instruction("add x5, x5, #1");                                          // next slot
    emitter.instruction("ldr x2, [x0, x5, lsl #3]");                               // load string len

    // -- JSON-encode this string (writes to concat_buf and updates concat_off) --
    // But we're already writing to concat_buf! We need to write inline instead.
    // Write opening quote, copy with escaping, write closing quote.

    // -- write opening quote --
    emitter.instruction("ldr x11, [sp, #16]");                                      // reload write pos
    emitter.instruction("mov w12, #34");                                            // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                         // write '"'
    emitter.instruction("add x11, x11, #1");                                        // advance

    // -- copy string bytes (simplified: no escaping for now, similar to json_encode_str inline) --
    emitter.instruction("mov x10, #0");                                             // copy index
    emitter.label("__rt_json_arr_str_copy");
    emitter.instruction("cmp x10, x2");                                             // check if all bytes copied
    emitter.instruction("b.ge __rt_json_arr_str_quote_close");                      // done

    emitter.instruction("ldrb w12, [x1, x10]");                                     // load source byte

    // -- check for characters needing escape --
    emitter.instruction("cmp w12, #34");                                            // check for double quote
    emitter.instruction("b.eq __rt_json_arr_str_esc2");                             // escape it
    emitter.instruction("cmp w12, #92");                                            // check for backslash
    emitter.instruction("b.eq __rt_json_arr_str_esc2");                             // escape it
    emitter.instruction("cmp w12, #10");                                            // check for newline
    emitter.instruction("b.eq __rt_json_arr_str_esc_n");                            // escape it

    // -- regular char --
    emitter.instruction("strb w12, [x11]");                                         // write char
    emitter.instruction("add x11, x11, #1");                                        // advance
    emitter.instruction("add x10, x10, #1");                                        // next source char
    emitter.instruction("b __rt_json_arr_str_copy");                                // continue

    // -- escape: write backslash + char --
    emitter.label("__rt_json_arr_str_esc2");
    emitter.instruction("mov w13, #92");                                            // backslash
    emitter.instruction("strb w13, [x11]");                                         // write backslash
    emitter.instruction("strb w12, [x11, #1]");                                     // write the char itself
    emitter.instruction("add x11, x11, #2");                                        // advance by 2
    emitter.instruction("add x10, x10, #1");                                        // next source char
    emitter.instruction("b __rt_json_arr_str_copy");                                // continue

    emitter.label("__rt_json_arr_str_esc_n");
    emitter.instruction("mov w13, #92");                                            // backslash
    emitter.instruction("strb w13, [x11]");                                         // write backslash
    emitter.instruction("mov w13, #110");                                           // 'n'
    emitter.instruction("strb w13, [x11, #1]");                                     // write 'n'
    emitter.instruction("add x11, x11, #2");                                        // advance by 2
    emitter.instruction("add x10, x10, #1");                                        // next source char
    emitter.instruction("b __rt_json_arr_str_copy");                                // continue

    // -- write closing quote --
    emitter.label("__rt_json_arr_str_quote_close");
    emitter.instruction("mov w12, #34");                                            // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                         // write '"'
    emitter.instruction("add x11, x11, #1");                                        // advance
    emitter.instruction("str x11, [sp, #16]");                                      // save write pos

    // -- advance to next element --
    emitter.instruction("ldr x4, [sp, #32]");                                       // reload index
    emitter.instruction("add x4, x4, #1");                                          // increment
    emitter.instruction("str x4, [sp, #32]");                                       // save index
    emitter.instruction("b __rt_json_arr_str_loop");                                // continue loop

    // -- write closing bracket --
    emitter.label("__rt_json_arr_str_close");
    emitter.instruction("ldr x11, [sp, #16]");                                      // reload write pos
    emitter.instruction("mov w12, #93");                                            // ASCII ']'
    emitter.instruction("strb w12, [x11]");                                         // write ']'
    emitter.instruction("add x11, x11, #1");                                        // advance

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #8]");                                        // x1 = output start
    emitter.instruction("sub x2, x11, x1");                                         // x2 = total length

    // -- update concat_off --
    emitter.instruction("adrp x9, _concat_off@PAGE");                               // load page of concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                         // resolve address
    emitter.instruction("ldr x10, [x9]");                                            // load current offset
    emitter.instruction("add x10, x10, x2");                                        // add result length
    emitter.instruction("str x10, [x9]");                                            // store updated offset

    // -- tear down and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                                 // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                          // deallocate stack frame
    emitter.instruction("ret");                                                     // return to caller
}

/// __rt_json_encode_assoc: encode an assoc array as JSON '{"key":"value",...}'.
/// Input:  x0 = hash table pointer
/// Output: x1 = result ptr (in concat_buf), x2 = result len
///
/// Uses __rt_hash_iter to iterate the hash table entries.
/// Hash table iter yields: x1=key_ptr, x2=key_len, x3=val_ptr, x4=val_len per entry.
pub fn emit_json_encode_assoc(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_assoc ---");
    emitter.label("__rt_json_encode_assoc");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #96");                                         // allocate 96 bytes
    emitter.instruction("stp x29, x30, [sp, #80]");                                 // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                        // set new frame pointer

    // -- save hash table pointer --
    emitter.instruction("str x0, [sp, #0]");                                        // save hash ptr

    // -- get output position in concat_buf --
    emitter.instruction("adrp x9, _concat_off@PAGE");                               // load page of concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                         // resolve address
    emitter.instruction("ldr x10, [x9]");                                            // load current offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                              // load page of concat buffer
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                       // resolve address
    emitter.instruction("add x11, x11, x10");                                       // output position
    emitter.instruction("str x11, [sp, #8]");                                       // save output start
    emitter.instruction("str x11, [sp, #16]");                                      // save output write pos

    // -- write opening brace --
    emitter.instruction("mov w12, #123");                                           // ASCII '{'
    emitter.instruction("strb w12, [x11]");                                         // write '{'
    emitter.instruction("add x11, x11, #1");                                        // advance
    emitter.instruction("str x11, [sp, #16]");                                      // save write pos

    // -- get hash table count --
    emitter.instruction("ldr x0, [sp, #0]");                                        // reload hash ptr
    emitter.instruction("bl __rt_hash_count");                                      // get count → x0
    emitter.instruction("str x0, [sp, #24]");                                       // save count
    emitter.instruction("str xzr, [sp, #32]");                                      // iterator index = 0
    emitter.instruction("str xzr, [sp, #40]");                                      // items written = 0

    // -- iterate hash table entries --
    emitter.label("__rt_json_assoc_loop");
    emitter.instruction("ldr x4, [sp, #40]");                                       // load items written
    emitter.instruction("ldr x3, [sp, #24]");                                       // load total count
    emitter.instruction("cmp x4, x3");                                              // check if all items written
    emitter.instruction("b.ge __rt_json_assoc_close");                              // done

    // -- get next entry via hash_iter --
    emitter.instruction("ldr x0, [sp, #0]");                                        // reload hash ptr
    emitter.instruction("ldr x1, [sp, #32]");                                       // load iterator index
    emitter.instruction("bl __rt_hash_iter_next");                                  // get entry → x0=new_idx, x1=key_ptr, x2=key_len, x3=val_ptr, x4=val_len
    emitter.instruction("str x0, [sp, #32]");                                       // save new iterator index

    // -- save key and value on stack --
    emitter.instruction("str x1, [sp, #48]");                                       // save key ptr
    emitter.instruction("str x2, [sp, #56]");                                       // save key len
    emitter.instruction("str x3, [sp, #64]");                                       // save val ptr
    emitter.instruction("str x4, [sp, #72]");                                       // save val len

    // -- add comma if not first entry --
    emitter.instruction("ldr x5, [sp, #40]");                                       // load items written
    emitter.instruction("cbz x5, __rt_json_assoc_key");                             // skip comma for first
    emitter.instruction("ldr x11, [sp, #16]");                                      // reload write pos
    emitter.instruction("mov w12, #44");                                            // ASCII ','
    emitter.instruction("strb w12, [x11]");                                         // write ','
    emitter.instruction("add x11, x11, #1");                                        // advance
    emitter.instruction("str x11, [sp, #16]");                                      // save write pos

    // -- write key as quoted string --
    emitter.label("__rt_json_assoc_key");
    emitter.instruction("ldr x11, [sp, #16]");                                      // reload write pos
    emitter.instruction("mov w12, #34");                                            // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                         // write opening quote
    emitter.instruction("add x11, x11, #1");                                        // advance
    // -- copy key bytes --
    emitter.instruction("ldr x1, [sp, #48]");                                       // load key ptr
    emitter.instruction("ldr x2, [sp, #56]");                                       // load key len
    emitter.instruction("mov x10, #0");                                             // copy index
    emitter.label("__rt_json_assoc_key_copy");
    emitter.instruction("cmp x10, x2");                                             // check if done
    emitter.instruction("b.ge __rt_json_assoc_key_done");                           // done
    emitter.instruction("ldrb w12, [x1, x10]");                                     // load key byte
    emitter.instruction("strb w12, [x11, x10]");                                    // write to output
    emitter.instruction("add x10, x10, #1");                                        // increment
    emitter.instruction("b __rt_json_assoc_key_copy");                              // continue
    emitter.label("__rt_json_assoc_key_done");
    emitter.instruction("add x11, x11, x2");                                        // advance write pos
    emitter.instruction("mov w12, #34");                                            // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                         // write closing quote
    emitter.instruction("add x11, x11, #1");                                        // advance

    // -- write colon --
    emitter.instruction("mov w12, #58");                                            // ASCII ':'
    emitter.instruction("strb w12, [x11]");                                         // write ':'
    emitter.instruction("add x11, x11, #1");                                        // advance

    // -- write value as quoted string --
    emitter.instruction("mov w12, #34");                                            // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                         // write opening quote
    emitter.instruction("add x11, x11, #1");                                        // advance
    // -- copy value bytes --
    emitter.instruction("ldr x1, [sp, #64]");                                       // load val ptr
    emitter.instruction("ldr x2, [sp, #72]");                                       // load val len
    emitter.instruction("mov x10, #0");                                             // copy index
    emitter.label("__rt_json_assoc_val_copy");
    emitter.instruction("cmp x10, x2");                                             // check if done
    emitter.instruction("b.ge __rt_json_assoc_val_done");                           // done
    emitter.instruction("ldrb w12, [x1, x10]");                                     // load val byte
    emitter.instruction("strb w12, [x11, x10]");                                    // write to output
    emitter.instruction("add x10, x10, #1");                                        // increment
    emitter.instruction("b __rt_json_assoc_val_copy");                              // continue
    emitter.label("__rt_json_assoc_val_done");
    emitter.instruction("add x11, x11, x2");                                        // advance write pos
    emitter.instruction("mov w12, #34");                                            // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                         // write closing quote
    emitter.instruction("add x11, x11, #1");                                        // advance
    emitter.instruction("str x11, [sp, #16]");                                      // save write pos

    // -- increment items written --
    emitter.instruction("ldr x5, [sp, #40]");                                       // load items written
    emitter.instruction("add x5, x5, #1");                                          // increment
    emitter.instruction("str x5, [sp, #40]");                                       // save items written
    emitter.instruction("b __rt_json_assoc_loop");                                  // continue loop

    // -- write closing brace --
    emitter.label("__rt_json_assoc_close");
    emitter.instruction("ldr x11, [sp, #16]");                                      // reload write pos
    emitter.instruction("mov w12, #125");                                           // ASCII '}'
    emitter.instruction("strb w12, [x11]");                                         // write '}'
    emitter.instruction("add x11, x11, #1");                                        // advance

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #8]");                                        // x1 = output start
    emitter.instruction("sub x2, x11, x1");                                         // x2 = total length

    // -- update concat_off --
    emitter.instruction("adrp x9, _concat_off@PAGE");                               // load page of concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                         // resolve address
    emitter.instruction("ldr x10, [x9]");                                            // load current offset
    emitter.instruction("add x10, x10, x2");                                        // add result length
    emitter.instruction("str x10, [x9]");                                            // store updated offset

    // -- tear down and return --
    emitter.instruction("ldp x29, x30, [sp, #80]");                                 // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                          // deallocate stack frame
    emitter.instruction("ret");                                                     // return to caller
}

/// Emit JSON string constants for the data section.
pub fn emit_json_data() -> String {
    let mut out = String::new();
    out.push_str("_json_true:\n    .ascii \"true\"\n");
    out.push_str("_json_false:\n    .ascii \"false\"\n");
    out.push_str("_json_null:\n    .ascii \"null\"\n");
    out
}
