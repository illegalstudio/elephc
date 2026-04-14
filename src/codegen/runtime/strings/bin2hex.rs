use crate::codegen::{emit::Emitter, platform::Arch};

/// bin2hex: convert binary string to hex representation.
/// Input: x1/x2=string. Output: x1/x2=result (2x length).
pub fn emit_bin2hex(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_bin2hex_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: bin2hex ---");
    emitter.label_global("__rt_bin2hex");

    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining count

    emitter.label("__rt_bin2hex_loop");
    emitter.instruction("cbz x11, __rt_bin2hex_done");                          // done if no bytes left
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    // -- high nibble --
    emitter.instruction("lsr w13, w12, #4");                                    // extract high 4 bits
    emitter.instruction("cmp w13, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_bin2hex_hi_af");                             // yes → use a-f
    emitter.instruction("add w13, w13, #48");                                   // convert 0-9 to '0'-'9'
    emitter.instruction("b __rt_bin2hex_hi_store");                             // store
    emitter.label("__rt_bin2hex_hi_af");
    emitter.instruction("add w13, w13, #87");                                   // convert 10-15 to 'a'-'f'
    emitter.label("__rt_bin2hex_hi_store");
    emitter.instruction("strb w13, [x9], #1");                                  // write high nibble hex char
    // -- low nibble --
    emitter.instruction("and w13, w12, #0xf");                                  // extract low 4 bits
    emitter.instruction("cmp w13, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_bin2hex_lo_af");                             // yes → use a-f
    emitter.instruction("add w13, w13, #48");                                   // convert 0-9 to '0'-'9'
    emitter.instruction("b __rt_bin2hex_lo_store");                             // store
    emitter.label("__rt_bin2hex_lo_af");
    emitter.instruction("add w13, w13, #87");                                   // convert 10-15 to 'a'-'f'
    emitter.label("__rt_bin2hex_lo_store");
    emitter.instruction("strb w13, [x9], #1");                                  // write low nibble hex char
    emitter.instruction("b __rt_bin2hex_loop");                                 // next byte

    emitter.label("__rt_bin2hex_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

fn emit_bin2hex_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: bin2hex ---");
    emitter.label_global("__rt_bin2hex");

    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // load the current concat-buffer offset before appending the hexadecimal bytes
    emitter.instruction("lea r9, [rip + _concat_buf]");                         // load the base address of the shared concat buffer
    emitter.instruction("add r9, r8");                                          // compute the destination pointer at the current concat-buffer tail
    emitter.instruction("mov r10, r9");                                         // preserve the hexadecimal string start pointer for the return value
    emitter.instruction("mov rcx, rdx");                                        // copy the source byte count into a decrementing loop counter
    emitter.instruction("mov rsi, rax");                                        // copy the source pointer into a cursor register for byte-by-byte reads

    emitter.label("__rt_bin2hex_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been converted to two hex characters
    emitter.instruction("je __rt_bin2hex_done_linux_x86_64");                   // finish when the binary source string has been fully consumed
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the next source byte and widen it for nibble extraction
    emitter.instruction("add rsi, 1");                                          // advance the source cursor after consuming one byte
    emitter.instruction("sub rcx, 1");                                          // record that one source byte has been consumed

    emitter.instruction("mov edx, eax");                                        // copy the source byte into a scratch register for the high nibble
    emitter.instruction("shr edx, 4");                                          // isolate the high nibble from the source byte
    emitter.instruction("cmp edx, 10");                                         // decide whether the high nibble maps to 0-9 or a-f
    emitter.instruction("jge __rt_bin2hex_hi_af_linux_x86_64");                 // branch when the high nibble must be rendered as a-f
    emitter.instruction("add edx, 48");                                         // map high nibble 0-9 to ASCII '0'-'9'
    emitter.instruction("jmp __rt_bin2hex_hi_store_linux_x86_64");              // skip the a-f conversion once the numeric digit is ready
    emitter.label("__rt_bin2hex_hi_af_linux_x86_64");
    emitter.instruction("add edx, 87");                                         // map high nibble 10-15 to ASCII 'a'-'f'
    emitter.label("__rt_bin2hex_hi_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9], dl");                               // write the high nibble as the first hexadecimal character
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing the first hex character

    emitter.instruction("mov edx, eax");                                        // copy the source byte into a scratch register for the low nibble
    emitter.instruction("and edx, 15");                                         // isolate the low nibble from the source byte
    emitter.instruction("cmp edx, 10");                                         // decide whether the low nibble maps to 0-9 or a-f
    emitter.instruction("jge __rt_bin2hex_lo_af_linux_x86_64");                 // branch when the low nibble must be rendered as a-f
    emitter.instruction("add edx, 48");                                         // map low nibble 0-9 to ASCII '0'-'9'
    emitter.instruction("jmp __rt_bin2hex_lo_store_linux_x86_64");              // skip the a-f conversion once the numeric digit is ready
    emitter.label("__rt_bin2hex_lo_af_linux_x86_64");
    emitter.instruction("add edx, 87");                                         // map low nibble 10-15 to ASCII 'a'-'f'
    emitter.label("__rt_bin2hex_lo_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9], dl");                               // write the low nibble as the second hexadecimal character
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing the second hex character
    emitter.instruction("jmp __rt_bin2hex_loop_linux_x86_64");                  // continue converting subsequent source bytes

    emitter.label("__rt_bin2hex_done_linux_x86_64");
    emitter.instruction("mov rax, r10");                                        // return the hexadecimal string start pointer in the standard x86_64 string result register
    emitter.instruction("mov rdx, r9");                                         // copy the concat-buffer tail into the length scratch register
    emitter.instruction("sub rdx, r10");                                        // compute the hexadecimal string length from the written byte count
    emitter.instruction("mov r8, r9");                                          // copy the absolute concat-buffer tail before normalizing it back to a shared offset
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // load the concat-buffer base so the shared offset can stay relative
    emitter.instruction("sub r8, r11");                                         // convert the absolute concat-buffer tail back into the shared relative offset
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r8");               // publish the updated relative concat-buffer offset for later string appenders
    emitter.instruction("ret");                                                 // return the hexadecimal string through the standard x86_64 string result registers
}
