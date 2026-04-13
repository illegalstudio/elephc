use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_encode_array_int: encode an int array as JSON "[1,2,3]".
/// Input:  x0 = array pointer (header: capacity[8], length[8], then elements[8 each])
/// Output: x1 = result ptr (in concat_buf), x2 = result len
pub(crate) fn emit_json_encode_array_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_array_int_linux_x86_64(emitter);
        return;
    }

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

fn emit_json_encode_array_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_int ---");
    emitter.label_global("__rt_json_encode_array_int");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving JSON-array scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source array and concat-buffer cursors
    emitter.instruction("sub rsp, 40");                                         // reserve local slots for the array pointer, output pointers, array length, and loop index
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source integer array pointer across itoa calls and concat-buffer copies
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer absolute offset before appending the JSON array
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the current JSON append
    emitter.instruction("add r11, r10");                                        // compute the current concat-buffer write pointer from the base plus offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the encoded-array start pointer for the final result slice
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the current concat-buffer write pointer for the element loop
    emitter.instruction("mov BYTE PTR [r11], 91");                              // write the opening JSON bracket before any encoded integer payloads
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the opening bracket
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer before entering the element loop
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the integer-array length from the first header field
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the array length across itoa calls and concat-buffer copies
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the integer-array element loop counter to zero

    emitter.label("__rt_json_arr_int_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current integer-array element index at the top of the JSON loop
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // have we already encoded every integer-array element?
    emitter.instruction("jae __rt_json_arr_int_close");                         // finish by writing the closing bracket once the loop index reaches the array length
    emitter.instruction("test r10, r10");                                       // is this the first integer-array element in the JSON output?
    emitter.instruction("jz __rt_json_arr_int_elem");                           // skip the comma separator before the first encoded integer element
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current concat-buffer write pointer before appending a comma separator
    emitter.instruction("mov BYTE PTR [r11], 44");                              // write the JSON comma separator between encoded integer elements
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the comma separator
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer after appending the comma separator

    emitter.label("__rt_json_arr_int_elem");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source integer-array pointer before loading the current element payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current integer-array element index before computing the payload slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first integer payload slot
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the integer element payload from the indexed-array storage slot
    emitter.instruction("call __rt_itoa");                                      // encode the integer element as a decimal JSON slice
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current concat-buffer write pointer before copying the encoded integer bytes
    emitter.instruction("xor rcx, rcx");                                        // initialize the encoded-integer copy index to the beginning of the returned decimal slice

    emitter.label("__rt_json_arr_int_copy");
    emitter.instruction("cmp rcx, rdx");                                        // have we copied every byte of the returned decimal slice?
    emitter.instruction("jae __rt_json_arr_int_next");                          // finish copying once the decimal slice length has been exhausted
    emitter.instruction("mov r10b, BYTE PTR [rax + rcx]");                      // load the next encoded decimal byte from the returned slice
    emitter.instruction("mov BYTE PTR [r11 + rcx], r10b");                      // copy the encoded decimal byte into concat_buf at the current write position
    emitter.instruction("add rcx, 1");                                          // advance the encoded-integer copy index to the next byte
    emitter.instruction("jmp __rt_json_arr_int_copy");                          // continue copying until the whole decimal slice has been appended

    emitter.label("__rt_json_arr_int_next");
    emitter.instruction("add r11, rdx");                                        // advance the concat-buffer write pointer by the copied decimal slice length
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer after appending the encoded integer
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // advance the integer-array element loop counter to the next payload slot
    emitter.instruction("jmp __rt_json_arr_int_loop");                          // continue encoding the remaining integer-array elements

    emitter.label("__rt_json_arr_int_close");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the concat-buffer write pointer after the final encoded integer element
    emitter.instruction("mov BYTE PTR [r11], 93");                              // append the closing JSON bracket to complete the encoded integer array slice
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the closing bracket
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the encoded-array start pointer in the leading x86_64 string result register
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer write pointer before turning it into a slice length
    emitter.instruction("sub rdx, rax");                                        // compute the final encoded-array length from write_end - write_start
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the global offset update
    emitter.instruction("mov rcx, r11");                                        // copy the final concat-buffer write pointer before converting it into an absolute offset
    emitter.instruction("sub rcx, r10");                                        // compute the new absolute concat-buffer offset after the encoded JSON array
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the updated concat-buffer offset so later writers append after this JSON array
    emitter.instruction("add rsp, 40");                                         // release the local JSON-array scratch frame before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the encoded JSON integer array slice in the x86_64 string result registers
}
