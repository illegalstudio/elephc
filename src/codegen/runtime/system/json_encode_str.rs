use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_encode_str: JSON-encode a string (add quotes, escape special chars).
/// Input:  x1 = string ptr, x2 = string len
/// Output: x1 = result ptr (in concat_buf), x2 = result len
pub(crate) fn emit_json_encode_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_str_linux_x86_64(emitter);
        return;
    }

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

fn emit_json_encode_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_str ---");
    emitter.label_global("__rt_json_encode_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving JSON-string scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source slice and concat-buffer cursors
    emitter.instruction("sub rsp, 48");                                         // reserve local slots for source ptr/len, output start, write pointer, and source index
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source string pointer across the JSON escaping loop
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source string length across the JSON escaping loop
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer absolute offset before appending the JSON string
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the current JSON append
    emitter.instruction("add r11, r10");                                        // compute the current concat-buffer write pointer from the base plus offset
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the encoded-string start pointer for the final result slice
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the current concat-buffer write pointer for the escape loop
    emitter.instruction("mov BYTE PTR [r11], 34");                              // write the opening JSON quote before any escaped payload bytes
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the opening quote
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer before entering the source-byte loop
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the source-byte index to the beginning of the input string

    emitter.label("__rt_json_str_loop");
    emitter.instruction("mov r13, QWORD PTR [rbp - 40]");                       // reload the current source-byte index at the top of the JSON escape loop
    emitter.instruction("cmp r13, QWORD PTR [rbp - 16]");                       // have we consumed every byte of the source string?
    emitter.instruction("jae __rt_json_str_close");                             // finish by writing the closing quote once the whole source slice has been escaped
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source string pointer for the current byte fetch
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the current concat-buffer write pointer before appending the next escaped byte
    emitter.instruction("movzx r14, BYTE PTR [r10 + r13]");                     // load the next source byte and widen it so escape comparisons stay unsigned
    emitter.instruction("cmp r14b, 34");                                        // does the source byte equal a JSON double quote?
    emitter.instruction("je __rt_json_str_esc_quote");                          // escape embedded double quotes as \\"
    emitter.instruction("cmp r14b, 92");                                        // does the source byte equal a backslash?
    emitter.instruction("je __rt_json_str_esc_backslash");                      // escape embedded backslashes as \\\\
    emitter.instruction("cmp r14b, 10");                                        // does the source byte equal a newline?
    emitter.instruction("je __rt_json_str_esc_n");                              // escape newlines as \\n
    emitter.instruction("cmp r14b, 13");                                        // does the source byte equal a carriage return?
    emitter.instruction("je __rt_json_str_esc_r");                              // escape carriage returns as \\r
    emitter.instruction("cmp r14b, 9");                                         // does the source byte equal a horizontal tab?
    emitter.instruction("je __rt_json_str_esc_t");                              // escape tabs as \\t
    emitter.instruction("mov BYTE PTR [r11], r14b");                            // copy ordinary bytes directly into the concat buffer without any escape expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer after the copied ordinary byte
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after copying the ordinary byte
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after copying the ordinary byte
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_quote");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the escape backslash that prefixes an embedded JSON quote
    emitter.instruction("mov BYTE PTR [r11 + 1], 34");                          // write the escaped JSON quote after the backslash prefix
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the two-byte escape sequence
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the quote escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the embedded quote
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_backslash");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the first backslash of the escaped backslash pair
    emitter.instruction("mov BYTE PTR [r11 + 1], 92");                          // write the second backslash of the escaped backslash pair
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the escaped backslash pair
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the backslash escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the backslash
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_n");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the escape backslash that prefixes a JSON newline escape
    emitter.instruction("mov BYTE PTR [r11 + 1], 110");                         // write the JSON newline escape codepoint n
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the two-byte newline escape
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the newline escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the newline
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_r");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the escape backslash that prefixes a JSON carriage-return escape
    emitter.instruction("mov BYTE PTR [r11 + 1], 114");                         // write the JSON carriage-return escape codepoint r
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the two-byte carriage-return escape
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the carriage-return escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the carriage return
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_t");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the escape backslash that prefixes a JSON tab escape
    emitter.instruction("mov BYTE PTR [r11 + 1], 116");                         // write the JSON tab escape codepoint t
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the two-byte tab escape
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the tab escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the tab
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_close");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the concat-buffer write pointer after the final escaped payload byte
    emitter.instruction("mov BYTE PTR [r11], 34");                              // append the closing JSON quote to complete the encoded string slice
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the closing quote
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the encoded-string start pointer in the leading x86_64 string result register
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer write pointer before turning it into a slice length
    emitter.instruction("sub rdx, rax");                                        // compute the final encoded-string length from write_end - write_start
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the global offset update
    emitter.instruction("mov rcx, r11");                                        // copy the final concat-buffer write pointer before converting it into an absolute offset
    emitter.instruction("sub rcx, r10");                                        // compute the new absolute concat-buffer offset after the encoded JSON string
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the updated concat-buffer offset so nested writers append after this JSON string
    emitter.instruction("add rsp, 48");                                         // release the local JSON-string scratch frame before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the encoded JSON string slice in the x86_64 string result registers
}
