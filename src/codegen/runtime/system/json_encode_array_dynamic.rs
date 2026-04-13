use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_encode_array_dynamic: encode an indexed array by inspecting its packed value_type tag.
/// Input:  x0 = array pointer
/// Output: x1 = result ptr, x2 = result len
pub(crate) fn emit_json_encode_array_dynamic(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_array_dynamic_linux_x86_64(emitter);
        return;
    }

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

fn emit_json_encode_array_dynamic_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_dynamic ---");
    emitter.label_global("__rt_json_encode_array_dynamic");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving JSON-array scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for array metadata and concat-buffer cursors
    emitter.instruction("sub rsp, 48");                                         // reserve local slots for the array pointer, output pointers, length, value_type, and loop index
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source array pointer across nested JSON helper calls
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer offset before appending the JSON array
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the current JSON append
    emitter.instruction("add r11, r10");                                        // compute the current concat-buffer write pointer from the base plus offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the encoded-array start pointer for the final result slice
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the current concat-buffer write pointer for the element loop
    emitter.instruction("mov BYTE PTR [r11], 91");                              // write the opening JSON bracket before any encoded element payload
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the opening bracket
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer before entering the element loop
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the indexed-array length from the first field of the array header
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the array length across nested JSON helper calls
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed array kind word so the value_type tag can drive JSON dispatch
    emitter.instruction("shr r10, 8");                                          // move the packed array value_type tag into the low bits for x86_64 dispatch
    emitter.instruction("and r10, 0x7f");                                       // isolate the packed array value_type tag without the persistent COW flag
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the packed array value_type tag across nested JSON helper calls
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the indexed-array element loop counter to zero

    emitter.label("__rt_json_arr_dyn_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index at the top of the JSON loop
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // have we already encoded every indexed-array element?
    emitter.instruction("jae __rt_json_arr_dyn_close");                         // finish by writing the closing bracket once the loop index reaches the array length
    emitter.instruction("test r10, r10");                                       // is this the first indexed-array element in the JSON output?
    emitter.instruction("jz __rt_json_arr_dyn_elem");                           // skip the comma separator before the first encoded element
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current concat-buffer write pointer before appending a comma separator
    emitter.instruction("mov BYTE PTR [r11], 44");                              // write the JSON comma separator between encoded array elements
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the comma separator
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer after appending the comma separator

    emitter.label("__rt_json_arr_dyn_elem");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current concat-buffer write pointer before a nested JSON helper appends data
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the global offset update
    emitter.instruction("mov rcx, r11");                                        // copy the current write pointer before turning it into an absolute concat offset
    emitter.instruction("sub rcx, r10");                                        // compute the concat-buffer absolute offset for the current write position
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the concat-buffer offset so nested JSON helpers append after the existing prefix
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the packed indexed-array value_type tag for runtime JSON dispatch
    emitter.instruction("cmp r10, 0");                                          // does this indexed array store integers?
    emitter.instruction("je __rt_json_arr_dyn_value_int");                      // encode integer elements through the decimal integer helper
    emitter.instruction("cmp r10, 1");                                          // does this indexed array store strings?
    emitter.instruction("je __rt_json_arr_dyn_value_str");                      // encode string elements through the JSON string helper
    emitter.instruction("cmp r10, 2");                                          // does this indexed array store floats?
    emitter.instruction("je __rt_json_arr_dyn_value_float");                    // encode float elements through the decimal float helper
    emitter.instruction("cmp r10, 3");                                          // does this indexed array store bools?
    emitter.instruction("je __rt_json_arr_dyn_value_bool");                     // encode bool elements through the JSON bool helper
    emitter.instruction("cmp r10, 4");                                          // does this indexed array store nested indexed arrays?
    emitter.instruction("je __rt_json_arr_dyn_value_array");                    // encode nested indexed arrays recursively
    emitter.instruction("cmp r10, 5");                                          // does this indexed array store nested associative arrays?
    emitter.instruction("je __rt_json_arr_dyn_value_assoc");                    // encode nested associative arrays recursively
    emitter.instruction("cmp r10, 7");                                          // does this indexed array store boxed mixed payloads?
    emitter.instruction("je __rt_json_arr_dyn_value_mixed");                    // encode boxed mixed payloads through the mixed JSON helper
    emitter.instruction("jmp __rt_json_arr_dyn_value_null");                    // unsupported object-like payloads currently degrade to JSON null

    emitter.label("__rt_json_arr_dyn_value_int");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the integer element payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the payload slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the integer element payload from the indexed-array storage slot
    emitter.instruction("call __rt_itoa");                                      // encode the integer element as a decimal JSON slice
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_str");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the ptr/len pair slots
    emitter.instruction("mov rcx, r10");                                        // copy the current indexed-array element index before scaling it into a ptr/len slot pair
    emitter.instruction("add rcx, rcx");                                        // compute index * 2 because string arrays store pointer/length pairs
    emitter.instruction("add rcx, 3");                                          // skip the 24-byte indexed-array header to land on the first ptr/len slot pair
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the string ptr/len pair
    emitter.instruction("mov rax, QWORD PTR [r10 + rcx * 8]");                  // load the string pointer from the indexed-array ptr/len storage pair
    emitter.instruction("add rcx, 1");                                          // advance from the string pointer slot to the paired string length slot
    emitter.instruction("mov rdx, QWORD PTR [r10 + rcx * 8]");                  // load the string length from the indexed-array ptr/len storage pair
    emitter.instruction("call __rt_json_encode_str");                           // encode the string element with JSON escaping and quotes
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_float");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the float payload bits
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the float slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov r10, QWORD PTR [rax + r10 * 8]");                  // load the raw float bit-pattern from the indexed-array storage slot
    emitter.instruction("movq xmm0, r10");                                      // move the raw float bit-pattern into the x86_64 floating-point argument register
    emitter.instruction("call __rt_ftoa");                                      // encode the float element as a decimal JSON slice
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_bool");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the bool payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the bool slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the bool payload from the indexed-array storage slot
    emitter.instruction("call __rt_json_encode_bool");                          // encode the bool element as the JSON literals true/false
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_array");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the nested indexed-array payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the nested-array slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the nested indexed-array pointer from the indexed-array storage slot
    emitter.instruction("call __rt_json_encode_array_dynamic");                 // encode the nested indexed-array recursively into a JSON slice
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded nested JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_assoc");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the nested associative-array payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the nested-hash slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the nested associative-array pointer from the indexed-array storage slot
    emitter.instruction("call __rt_json_encode_assoc");                         // encode the nested associative array recursively into a JSON slice
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded nested JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_mixed");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the boxed mixed payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the mixed payload slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the boxed mixed pointer from the indexed-array storage slot
    emitter.instruction("call __rt_json_encode_mixed");                         // encode the boxed mixed payload recursively into a JSON slice
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded nested JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_null");
    emitter.instruction("call __rt_json_encode_null");                          // encode null or unsupported payload families as the JSON null literal

    emitter.label("__rt_json_arr_dyn_copy");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current concat-buffer write pointer before copying the encoded element bytes
    emitter.instruction("xor rcx, rcx");                                        // initialize the encoded-element copy index to the beginning of the returned JSON slice
    emitter.label("__rt_json_arr_dyn_copy_loop");
    emitter.instruction("cmp rcx, rdx");                                        // have we copied every byte of the returned encoded JSON slice?
    emitter.instruction("jae __rt_json_arr_dyn_next");                          // finish copying once the slice length has been exhausted
    emitter.instruction("mov r10b, BYTE PTR [rax + rcx]");                      // load the next encoded JSON byte from the returned slice
    emitter.instruction("mov BYTE PTR [r11 + rcx], r10b");                      // copy the encoded JSON byte into concat_buf at the current write position
    emitter.instruction("add rcx, 1");                                          // advance the encoded-element copy index to the next byte
    emitter.instruction("jmp __rt_json_arr_dyn_copy_loop");                     // continue copying until the whole returned JSON slice has been appended

    emitter.label("__rt_json_arr_dyn_next");
    emitter.instruction("add r11, rdx");                                        // advance the concat-buffer write pointer by the copied encoded-element length
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer after appending the encoded element
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // advance the indexed-array element loop counter to the next payload slot
    emitter.instruction("jmp __rt_json_arr_dyn_loop");                          // continue encoding the remaining indexed-array elements

    emitter.label("__rt_json_arr_dyn_close");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the concat-buffer write pointer after the final encoded JSON element
    emitter.instruction("mov BYTE PTR [r11], 93");                              // append the closing JSON bracket to complete the encoded array slice
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the closing bracket
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the encoded-array start pointer in the leading x86_64 string result register
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer write pointer before turning it into a slice length
    emitter.instruction("sub rdx, rax");                                        // compute the final encoded-array length from write_end - write_start
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the global offset update
    emitter.instruction("mov rcx, r11");                                        // copy the final concat-buffer write pointer before converting it into an absolute offset
    emitter.instruction("sub rcx, r10");                                        // compute the new absolute concat-buffer offset after the encoded JSON array
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the updated concat-buffer offset so later writers append after this JSON array
    emitter.instruction("add rsp, 48");                                         // release the local JSON-array scratch frame before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the encoded JSON array slice in the x86_64 string result registers
}
