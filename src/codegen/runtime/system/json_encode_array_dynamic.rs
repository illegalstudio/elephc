use crate::codegen::emit::Emitter;

/// __rt_json_encode_array_dynamic: encode an indexed array by inspecting its packed value_type tag.
/// Input:  x0 = array pointer
/// Output: x1 = result ptr, x2 = result len
pub(crate) fn emit_json_encode_array_dynamic(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_dynamic ---");
    emitter.label_global("__rt_json_encode_array_dynamic");

    emitter.instruction("sub sp, sp, #112");                                    // allocate stack space for array metadata and element scratch values
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the helper stack frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the source array pointer

    // -- initialize concat buffer write pointers --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // compute the current write pointer
    emitter.instruction("str x11, [sp, #8]");                                   // save the output start pointer
    emitter.instruction("str x11, [sp, #16]");                                  // save the current write pointer

    // -- write opening bracket --
    emitter.instruction("mov w12, #91");                                        // ASCII '['
    emitter.instruction("strb w12, [x11]");                                     // write the opening bracket
    emitter.instruction("add x11, x11, #1");                                    // advance past the opening bracket
    emitter.instruction("str x11, [sp, #16]");                                  // persist the updated write pointer

    // -- cache array length and packed value_type tag --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load the current array length
    emitter.instruction("str x3, [sp, #24]");                                   // save the array length for the element loop
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed array kind word
    emitter.instruction("lsr x9, x9, #8");                                      // move the packed value_type tag into the low bits
    emitter.instruction("and x9, x9, #0x7f");                                   // isolate the packed value_type tag
    emitter.instruction("str x9, [sp, #32]");                                   // save the array element value_type tag
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize the loop index to zero

    emitter.label("__rt_json_arr_dyn_loop");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the array length
    emitter.instruction("cmp x4, x3");                                          // have we encoded every element?
    emitter.instruction("b.ge __rt_json_arr_dyn_close");                        // finish once the loop index reaches the array length

    // -- emit comma separators between elements --
    emitter.instruction("cbz x4, __rt_json_arr_dyn_elem");                      // skip the comma before the first element
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current write pointer
    emitter.instruction("mov w12, #44");                                        // ASCII ','
    emitter.instruction("strb w12, [x11]");                                     // write the comma separator
    emitter.instruction("add x11, x11, #1");                                    // advance past the comma
    emitter.instruction("str x11, [sp, #16]");                                  // persist the updated write pointer

    // -- update concat_off so nested encoders append from the current write position --
    emitter.label("__rt_json_arr_dyn_elem");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current write pointer
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x12, x11, x10");                                   // compute the absolute concat offset for the current write position
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x12, [x9]");                                       // nested encoders must append after the existing JSON prefix

    // -- dispatch on the array value_type tag --
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the packed array value_type tag
    emitter.instruction("cmp x12, #0");                                         // does this array store ints?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_int");                    // ints encode through itoa
    emitter.instruction("cmp x12, #1");                                         // does this array store strings?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_str");                    // strings encode through the JSON string helper
    emitter.instruction("cmp x12, #2");                                         // does this array store floats?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_float");                  // floats encode through ftoa
    emitter.instruction("cmp x12, #3");                                         // does this array store bools?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_bool");                   // bools encode through the JSON bool helper
    emitter.instruction("cmp x12, #4");                                         // does this array store nested indexed arrays?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_array");                  // nested arrays encode recursively
    emitter.instruction("cmp x12, #5");                                         // does this array store nested associative arrays?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_assoc");                  // nested hashes encode through the assoc helper
    emitter.instruction("cmp x12, #7");                                         // does this array store boxed mixed payloads?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_mixed");                  // boxed mixed payloads encode through the mixed helper
    emitter.instruction("b __rt_json_arr_dyn_value_null");                      // null and unsupported object payloads encode as JSON null

    emitter.label("__rt_json_arr_dyn_value_int");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load the integer element payload
    emitter.instruction("bl __rt_itoa");                                        // encode the integer element as decimal digits
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_str");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x5, x4, x4");                                      // compute index * 2 for the ptr/len pair
    emitter.instruction("add x5, x5, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x1, [x0, x5, lsl #3]");                            // load the string pointer
    emitter.instruction("add x5, x5, #1");                                      // advance to the string length slot
    emitter.instruction("ldr x2, [x0, x5, lsl #3]");                            // load the string length
    emitter.instruction("bl __rt_json_encode_str");                             // encode the string element with JSON escaping
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_float");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x9, [x0, x4, lsl #3]");                            // load the float bits from the 8-byte array slot
    emitter.instruction("fmov d0, x9");                                         // move the float bits into the FP register file
    emitter.instruction("bl __rt_ftoa");                                        // encode the float element as decimal digits
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_bool");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load the bool payload from the 8-byte array slot
    emitter.instruction("bl __rt_json_encode_bool");                            // encode the bool element as true/false
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_array");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load the nested array pointer from the 8-byte array slot
    emitter.instruction("bl __rt_json_encode_array_dynamic");                   // encode the nested indexed array recursively
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded nested array into concat_buf

    emitter.label("__rt_json_arr_dyn_value_assoc");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load the nested associative-array pointer from the 8-byte array slot
    emitter.instruction("bl __rt_json_encode_assoc");                           // encode the nested associative array recursively
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded nested hash into concat_buf

    emitter.label("__rt_json_arr_dyn_value_mixed");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load the boxed mixed pointer from the 8-byte array slot
    emitter.instruction("bl __rt_json_encode_mixed");                           // encode the boxed mixed payload recursively
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded mixed payload into concat_buf

    emitter.label("__rt_json_arr_dyn_value_null");
    emitter.instruction("bl __rt_json_encode_null");                            // encode null/unsupported payloads as JSON null

    emitter.label("__rt_json_arr_dyn_copy");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current concat_buf write pointer
    emitter.instruction("mov x10, #0");                                         // initialize the copy index for the encoded element
    emitter.label("__rt_json_arr_dyn_copy_loop");
    emitter.instruction("cmp x10, x2");                                         // have we copied every encoded byte?
    emitter.instruction("b.ge __rt_json_arr_dyn_next");                         // finish once the encoded element has been copied
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load the next encoded byte
    emitter.instruction("strb w12, [x11, x10]");                                // write the encoded byte into concat_buf
    emitter.instruction("add x10, x10, #1");                                    // advance the copy index
    emitter.instruction("b __rt_json_arr_dyn_copy_loop");                       // continue copying the encoded element

    emitter.label("__rt_json_arr_dyn_next");
    emitter.instruction("add x11, x11, x2");                                    // advance the concat_buf write pointer by the encoded element length
    emitter.instruction("str x11, [sp, #16]");                                  // persist the updated concat_buf write pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #1");                                      // advance to the next array element
    emitter.instruction("str x4, [sp, #40]");                                   // persist the updated loop index
    emitter.instruction("b __rt_json_arr_dyn_loop");                            // continue encoding the remaining elements

    emitter.label("__rt_json_arr_dyn_close");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current concat_buf write pointer
    emitter.instruction("mov w12, #93");                                        // ASCII ']'
    emitter.instruction("strb w12, [x11]");                                     // write the closing bracket
    emitter.instruction("add x11, x11, #1");                                    // advance past the closing bracket
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the output start pointer
    emitter.instruction("sub x2, x11, x1");                                     // compute the total encoded array length
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x10, x11, x10");                                   // compute the absolute concat offset after the closing bracket
    emitter.instruction("str x10, [x9]");                                       // persist the updated concat offset
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the helper stack frame
    emitter.instruction("ret");                                                 // return the encoded JSON slice in x1/x2
}
