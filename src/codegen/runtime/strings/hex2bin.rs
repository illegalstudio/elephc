use crate::codegen::{emit::Emitter, platform::Arch};

/// hex2bin: convert hex string to binary.
/// Input: x1/x2=hex_string. Output: x1/x2=result (half length).
pub fn emit_hex2bin(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hex2bin_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hex2bin ---");
    emitter.label_global("__rt_hex2bin");

    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining hex chars

    emitter.label("__rt_hex2bin_loop");
    emitter.instruction("cmp x11, #2");                                         // need at least 2 hex chars
    emitter.instruction("b.lt __rt_hex2bin_done");                              // not enough → done

    // -- parse high nibble (inline hex digit conversion) --
    emitter.instruction("ldrb w12, [x1], #1");                                  // load first hex char
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le 1f");                                             // yes → numeric
    emitter.instruction("cmp w12, #70");                                        // <= 'F'?
    emitter.instruction("b.le 2f");                                             // yes → uppercase
    emitter.instruction("sub w12, w12, #87");                                   // 'a'-'f' → 10-15
    emitter.instruction("b 3f");                                                // done with high nibble
    emitter.raw("1:");
    emitter.instruction("sub w12, w12, #48");                                   // '0'-'9' → 0-9
    emitter.instruction("b 3f");                                                // done
    emitter.raw("2:");
    emitter.instruction("sub w12, w12, #55");                                   // 'A'-'F' → 10-15
    emitter.raw("3:");
    emitter.instruction("lsl w13, w12, #4");                                    // shift to high nibble

    // -- parse low nibble (inline hex digit conversion) --
    emitter.instruction("ldrb w12, [x1], #1");                                  // load second hex char
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le 4f");                                             // yes → numeric
    emitter.instruction("cmp w12, #70");                                        // <= 'F'?
    emitter.instruction("b.le 5f");                                             // yes → uppercase
    emitter.instruction("sub w12, w12, #87");                                   // 'a'-'f' → 10-15
    emitter.instruction("b 6f");                                                // done with low nibble
    emitter.raw("4:");
    emitter.instruction("sub w12, w12, #48");                                   // '0'-'9' → 0-9
    emitter.instruction("b 6f");                                                // done
    emitter.raw("5:");
    emitter.instruction("sub w12, w12, #55");                                   // 'A'-'F' → 10-15
    emitter.raw("6:");
    emitter.instruction("orr w13, w13, w12");                                   // combine high and low nibbles
    emitter.instruction("strb w13, [x9], #1");                                  // store decoded byte
    emitter.instruction("sub x11, x11, #2");                                    // consumed 2 hex chars
    emitter.instruction("b __rt_hex2bin_loop");                                 // next pair

    emitter.label("__rt_hex2bin_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store
    emitter.instruction("ret");                                                 // return
}

fn emit_hex2bin_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hex2bin ---");
    emitter.label_global("__rt_hex2bin");

    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // load the current concat-buffer offset before appending the decoded bytes
    emitter.instruction("lea r9, [rip + _concat_buf]");                         // load the base address of the shared concat buffer
    emitter.instruction("add r9, r8");                                          // compute the destination pointer at the current concat-buffer tail
    emitter.instruction("mov r10, r9");                                         // preserve the decoded string start pointer for the return value
    emitter.instruction("mov rcx, rdx");                                        // copy the hexadecimal character count into a decrementing loop counter
    emitter.instruction("mov rsi, rax");                                        // copy the hexadecimal source pointer into a cursor register for byte-by-byte reads

    emitter.label("__rt_hex2bin_loop_linux_x86_64");
    emitter.instruction("cmp rcx, 2");                                          // check whether at least one complete hexadecimal pair remains
    emitter.instruction("jl __rt_hex2bin_done_linux_x86_64");                   // stop once fewer than two hexadecimal characters remain

    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the high-nibble hexadecimal character and widen it for conversion
    emitter.instruction("add rsi, 1");                                          // advance the source cursor past the high-nibble character
    emitter.instruction("cmp eax, 57");                                         // decide whether the high nibble is numeric or alphabetic
    emitter.instruction("jle __rt_hex2bin_hi_num_linux_x86_64");                // branch when the high nibble is in the '0'-'9' range
    emitter.instruction("cmp eax, 70");                                         // decide whether the high nibble is uppercase hexadecimal
    emitter.instruction("jle __rt_hex2bin_hi_upper_linux_x86_64");              // branch when the high nibble is in the 'A'-'F' range
    emitter.instruction("sub eax, 87");                                         // map lowercase 'a'-'f' to nibble values 10-15
    emitter.instruction("jmp __rt_hex2bin_hi_done_linux_x86_64");               // skip the alternate conversion branches after handling lowercase input
    emitter.label("__rt_hex2bin_hi_num_linux_x86_64");
    emitter.instruction("sub eax, 48");                                         // map numeric '0'-'9' to nibble values 0-9
    emitter.instruction("jmp __rt_hex2bin_hi_done_linux_x86_64");               // skip the uppercase conversion branch after handling numeric input
    emitter.label("__rt_hex2bin_hi_upper_linux_x86_64");
    emitter.instruction("sub eax, 55");                                         // map uppercase 'A'-'F' to nibble values 10-15
    emitter.label("__rt_hex2bin_hi_done_linux_x86_64");
    emitter.instruction("mov edx, eax");                                        // preserve the converted high nibble before parsing the low nibble
    emitter.instruction("shl edx, 4");                                          // move the converted high nibble into the upper half of the decoded output byte

    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the low-nibble hexadecimal character and widen it for conversion
    emitter.instruction("add rsi, 1");                                          // advance the source cursor past the low-nibble character
    emitter.instruction("cmp eax, 57");                                         // decide whether the low nibble is numeric or alphabetic
    emitter.instruction("jle __rt_hex2bin_lo_num_linux_x86_64");                // branch when the low nibble is in the '0'-'9' range
    emitter.instruction("cmp eax, 70");                                         // decide whether the low nibble is uppercase hexadecimal
    emitter.instruction("jle __rt_hex2bin_lo_upper_linux_x86_64");              // branch when the low nibble is in the 'A'-'F' range
    emitter.instruction("sub eax, 87");                                         // map lowercase 'a'-'f' to nibble values 10-15
    emitter.instruction("jmp __rt_hex2bin_lo_done_linux_x86_64");               // skip the alternate conversion branches after handling lowercase input
    emitter.label("__rt_hex2bin_lo_num_linux_x86_64");
    emitter.instruction("sub eax, 48");                                         // map numeric '0'-'9' to nibble values 0-9
    emitter.instruction("jmp __rt_hex2bin_lo_done_linux_x86_64");               // skip the uppercase conversion branch after handling numeric input
    emitter.label("__rt_hex2bin_lo_upper_linux_x86_64");
    emitter.instruction("sub eax, 55");                                         // map uppercase 'A'-'F' to nibble values 10-15
    emitter.label("__rt_hex2bin_lo_done_linux_x86_64");
    emitter.instruction("or edx, eax");                                         // combine the high and low nibbles into the decoded output byte
    emitter.instruction("mov BYTE PTR [r9], dl");                               // write the decoded byte to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing one decoded byte
    emitter.instruction("sub rcx, 2");                                          // record that one hexadecimal character pair has been consumed
    emitter.instruction("jmp __rt_hex2bin_loop_linux_x86_64");                  // continue decoding subsequent hexadecimal character pairs

    emitter.label("__rt_hex2bin_done_linux_x86_64");
    emitter.instruction("mov rax, r10");                                        // return the decoded string start pointer in the standard x86_64 string result register
    emitter.instruction("mov rdx, r9");                                         // copy the concat-buffer tail into the length scratch register
    emitter.instruction("sub rdx, r10");                                        // compute the decoded string length from the written byte count
    emitter.instruction("mov r8, r9");                                          // copy the absolute concat-buffer tail before normalizing it back to a shared offset
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // load the concat-buffer base so the shared offset can stay relative
    emitter.instruction("sub r8, r11");                                         // convert the absolute concat-buffer tail back into the shared relative offset
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r8");               // publish the updated relative concat-buffer offset for later string appenders
    emitter.instruction("ret");                                                 // return the decoded string through the standard x86_64 string result registers
}
