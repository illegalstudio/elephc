use crate::codegen::emit::Emitter;

/// __rt_json_encode_assoc: encode an assoc array as JSON '{"key":"value",...}'.
/// Input:  x0 = hash table pointer
/// Output: x1 = result ptr (in concat_buf), x2 = result len
///
/// Uses __rt_hash_iter_next to iterate the hash table entries in insertion order.
/// Hash table iter yields: x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi, x5=val_tag per entry.
pub(crate) fn emit_json_encode_assoc(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_assoc ---");
    emitter.label_global("__rt_json_encode_assoc");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #112");                                    // allocate 112 bytes
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // set new frame pointer

    // -- save hash table pointer --
    emitter.instruction("str x0, [sp, #0]");                                    // save hash ptr

    // -- get output position in concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // output position
    emitter.instruction("str x11, [sp, #8]");                                   // save output start
    emitter.instruction("str x11, [sp, #16]");                                  // save output write pos

    // -- write opening brace --
    emitter.instruction("mov w12, #123");                                       // ASCII '{'
    emitter.instruction("strb w12, [x11]");                                     // write '{'
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- get hash table count --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload hash ptr
    emitter.instruction("bl __rt_hash_count");                                  // get count → x0
    emitter.instruction("str x0, [sp, #24]");                                   // save count
    emitter.instruction("str xzr, [sp, #32]");                                  // iterator cursor = 0 (start from hash header head)
    emitter.instruction("str xzr, [sp, #40]");                                  // items written = 0

    // -- iterate hash table entries --
    emitter.label("__rt_json_assoc_loop");
    emitter.instruction("ldr x4, [sp, #40]");                                   // load items written
    emitter.instruction("ldr x3, [sp, #24]");                                   // load total count
    emitter.instruction("cmp x4, x3");                                          // check if all items written
    emitter.instruction("b.ge __rt_json_assoc_close");                          // done

    // -- get next entry via hash_iter --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload hash ptr
    emitter.instruction("ldr x1, [sp, #32]");                                   // load iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // get entry → x0=next_cursor, x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi
    emitter.instruction("str x0, [sp, #32]");                                   // save next iterator cursor

    // -- save key and value on stack --
    emitter.instruction("str x1, [sp, #48]");                                   // save key ptr
    emitter.instruction("str x2, [sp, #56]");                                   // save key len
    emitter.instruction("str x3, [sp, #64]");                                   // save val_lo
    emitter.instruction("str x4, [sp, #72]");                                   // save val_hi
    emitter.instruction("str x5, [sp, #88]");                                   // save val_tag

    // -- add comma if not first entry --
    emitter.instruction("ldr x5, [sp, #40]");                                   // load items written
    emitter.instruction("cbz x5, __rt_json_assoc_key");                         // skip comma for first
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #44");                                        // ASCII ','
    emitter.instruction("strb w12, [x11]");                                     // write ','
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- write key as quoted string --
    emitter.label("__rt_json_assoc_key");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // write opening quote
    emitter.instruction("add x11, x11, #1");                                    // advance

    // -- copy key bytes --
    emitter.instruction("ldr x1, [sp, #48]");                                   // load key ptr
    emitter.instruction("ldr x2, [sp, #56]");                                   // load key len
    emitter.instruction("mov x10, #0");                                         // copy index
    emitter.label("__rt_json_assoc_key_copy");
    emitter.instruction("cmp x10, x2");                                         // check if done
    emitter.instruction("b.ge __rt_json_assoc_key_done");                       // done
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load key byte
    emitter.instruction("strb w12, [x11, x10]");                                // write to output
    emitter.instruction("add x10, x10, #1");                                    // increment
    emitter.instruction("b __rt_json_assoc_key_copy");                          // continue
    emitter.label("__rt_json_assoc_key_done");
    emitter.instruction("add x11, x11, x2");                                    // advance write pos
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // write closing quote
    emitter.instruction("add x11, x11, #1");                                    // advance

    // -- write colon --
    emitter.instruction("mov w12, #58");                                        // ASCII ':'
    emitter.instruction("strb w12, [x11]");                                     // write ':'
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos after emitting the JSON key prefix

    // -- move concat_off to the current write position so nested encoders append safely --
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current output write position
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x12, x11, x10");                                   // x12 = absolute concat offset for the current write position
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x12, [x9]");                                       // nested JSON/string encoders append after the existing key prefix

    // -- encode the value according to its per-entry runtime tag --
    emitter.instruction("ldr x12, [sp, #88]");                                  // load the saved per-entry value_tag
    emitter.instruction("cmp x12, #0");                                         // is this value an integer?
    emitter.instruction("b.eq __rt_json_assoc_value_int");                      // encode integers via itoa
    emitter.instruction("cmp x12, #1");                                         // is this value a string?
    emitter.instruction("b.eq __rt_json_assoc_value_str");                      // encode strings with JSON escaping
    emitter.instruction("cmp x12, #2");                                         // is this value a float?
    emitter.instruction("b.eq __rt_json_assoc_value_float");                    // encode floats via ftoa
    emitter.instruction("cmp x12, #3");                                         // is this value a bool?
    emitter.instruction("b.eq __rt_json_assoc_value_bool");                     // encode bools via json_encode_bool
    emitter.instruction("cmp x12, #4");                                         // is this value an indexed array?
    emitter.instruction("b.eq __rt_json_assoc_value_array");                    // encode arrays via the indexed-array JSON helpers
    emitter.instruction("cmp x12, #5");                                         // is this value an associative array?
    emitter.instruction("b.eq __rt_json_assoc_value_assoc");                    // encode nested associative arrays recursively
    emitter.instruction("cmp x12, #8");                                         // is this value null?
    emitter.instruction("b.eq __rt_json_assoc_value_null");                     // encode null via json_encode_null
    emitter.instruction("b __rt_json_assoc_value_null");                        // unsupported mixed/object payloads currently encode as null

    emitter.label("__rt_json_assoc_value_int");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load integer payload from value_lo
    emitter.instruction("bl __rt_itoa");                                        // encode integer payload as decimal digits
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded value into concat_buf

    emitter.label("__rt_json_assoc_value_str");
    emitter.instruction("ldr x1, [sp, #64]");                                   // load string pointer from value_lo
    emitter.instruction("ldr x2, [sp, #72]");                                   // load string length from value_hi
    emitter.instruction("bl __rt_json_encode_str");                             // encode string payload with JSON escaping and quotes
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded value into concat_buf

    emitter.label("__rt_json_assoc_value_float");
    emitter.instruction("ldr x9, [sp, #64]");                                   // load float bits from value_lo
    emitter.instruction("fmov d0, x9");                                         // move float bits into the FP argument register
    emitter.instruction("bl __rt_ftoa");                                        // encode float payload as decimal digits
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded value into concat_buf

    emitter.label("__rt_json_assoc_value_bool");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load bool payload from value_lo
    emitter.instruction("bl __rt_json_encode_bool");                            // encode bool payload as true/false
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded value into concat_buf

    emitter.label("__rt_json_assoc_value_array");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load nested array pointer from value_lo
    emitter.instruction("bl __rt_json_encode_array_dynamic");                   // encode nested indexed arrays through the dynamic array JSON helper
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded nested array into concat_buf

    emitter.label("__rt_json_assoc_value_assoc");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load nested associative array pointer from value_lo
    emitter.instruction("bl __rt_json_encode_assoc");                           // encode the nested associative array recursively
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded nested associative array into concat_buf

    emitter.label("__rt_json_assoc_value_null");
    emitter.instruction("bl __rt_json_encode_null");                            // encode null or unsupported payloads as JSON null

    emitter.label("__rt_json_assoc_value_copy");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current concat_buf write position
    emitter.instruction("mov x10, #0");                                         // copy index
    emitter.label("__rt_json_assoc_val_copy");
    emitter.instruction("cmp x10, x2");                                         // check if done
    emitter.instruction("b.ge __rt_json_assoc_val_done");                       // done
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load val byte
    emitter.instruction("strb w12, [x11, x10]");                                // write to output
    emitter.instruction("add x10, x10, #1");                                    // increment
    emitter.instruction("b __rt_json_assoc_val_copy");                          // continue
    emitter.label("__rt_json_assoc_val_done");
    emitter.instruction("add x11, x11, x2");                                    // advance write pos
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- increment items written --
    emitter.instruction("ldr x5, [sp, #40]");                                   // load items written
    emitter.instruction("add x5, x5, #1");                                      // increment
    emitter.instruction("str x5, [sp, #40]");                                   // save items written
    emitter.instruction("b __rt_json_assoc_loop");                              // continue loop

    // -- write closing brace --
    emitter.label("__rt_json_assoc_close");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #125");                                       // ASCII '}'
    emitter.instruction("strb w12, [x11]");                                     // write '}'
    emitter.instruction("add x11, x11, #1");                                    // advance

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = output start
    emitter.instruction("sub x2, x11, x1");                                     // x2 = total length

    // -- update concat_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x10, x11, x10");                                   // compute the absolute concat offset after the closing brace
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- tear down and return --
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
