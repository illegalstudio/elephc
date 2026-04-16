use crate::codegen::{emit::Emitter, platform::Arch};

/// base64_encode: standard base64 encoding (3 input bytes -> 4 output chars).
/// Input: x1/x2=string. Output: x1/x2=result in concat_buf.
/// Uses _b64_encode_tbl data section for the lookup table.
pub fn emit_base64_encode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_base64_encode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: base64_encode ---");
    emitter.label_global("__rt_base64_encode");

    // -- set up concat_buf destination --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    // -- load base64 lookup table --
    crate::codegen::abi::emit_symbol_address(emitter, "x15", "_b64_encode_tbl");

    // -- process 3 bytes at a time --
    emitter.label("__rt_b64enc_loop");
    emitter.instruction("cmp x11, #3");                                         // at least 3 bytes left?
    emitter.instruction("b.lt __rt_b64enc_remainder");                          // no -> handle remainder

    // -- load 3 source bytes --
    emitter.instruction("ldrb w12, [x1], #1");                                  // byte 0
    emitter.instruction("ldrb w13, [x1], #1");                                  // byte 1
    emitter.instruction("ldrb w14, [x1], #1");                                  // byte 2
    emitter.instruction("sub x11, x11, #3");                                    // consumed 3 bytes

    // -- encode char 0: top 6 bits of byte 0 --
    emitter.instruction("lsr w16, w12, #2");                                    // byte0 >> 2
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup table[index]
    emitter.instruction("strb w16, [x9], #1");                                  // write encoded char 0

    // -- encode char 1: bottom 2 of byte0 + top 4 of byte1 --
    emitter.instruction("and w16, w12, #0x3");                                  // byte0 & 0x3
    emitter.instruction("lsl w16, w16, #4");                                    // shift left 4
    emitter.instruction("lsr w17, w13, #4");                                    // byte1 >> 4
    emitter.instruction("orr w16, w16, w17");                                   // combine
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup table[index]
    emitter.instruction("strb w16, [x9], #1");                                  // write encoded char 1

    // -- encode char 2: bottom 4 of byte1 + top 2 of byte2 --
    emitter.instruction("and w16, w13, #0xf");                                  // byte1 & 0xf
    emitter.instruction("lsl w16, w16, #2");                                    // shift left 2
    emitter.instruction("lsr w17, w14, #6");                                    // byte2 >> 6
    emitter.instruction("orr w16, w16, w17");                                   // combine
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup table[index]
    emitter.instruction("strb w16, [x9], #1");                                  // write encoded char 2

    // -- encode char 3: bottom 6 of byte2 --
    emitter.instruction("and w16, w14, #0x3f");                                 // byte2 & 0x3f
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup table[index]
    emitter.instruction("strb w16, [x9], #1");                                  // write encoded char 3

    emitter.instruction("b __rt_b64enc_loop");                                  // next 3 bytes

    // -- handle remainder (0, 1, or 2 bytes left) --
    emitter.label("__rt_b64enc_remainder");
    emitter.instruction("cbz x11, __rt_b64enc_done");                           // 0 bytes left -> done

    emitter.instruction("cmp x11, #1");                                         // exactly 1 byte left?
    emitter.instruction("b.ne __rt_b64enc_rem2");                               // no -> 2 bytes

    // -- 1 byte remainder: 2 encoded chars + 2 padding --
    emitter.instruction("ldrb w12, [x1]");                                      // load last byte
    // char 0: top 6 bits
    emitter.instruction("lsr w16, w12, #2");                                    // byte0 >> 2
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup
    emitter.instruction("strb w16, [x9], #1");                                  // write char 0
    // char 1: bottom 2 bits << 4
    emitter.instruction("and w16, w12, #0x3");                                  // byte0 & 0x3
    emitter.instruction("lsl w16, w16, #4");                                    // shift left 4
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup
    emitter.instruction("strb w16, [x9], #1");                                  // write char 1
    // padding
    emitter.instruction("mov w16, #61");                                        // '=' padding char
    emitter.instruction("strb w16, [x9], #1");                                  // write '='
    emitter.instruction("strb w16, [x9], #1");                                  // write '='
    emitter.instruction("b __rt_b64enc_done");                                  // done

    // -- 2 byte remainder: 3 encoded chars + 1 padding --
    emitter.label("__rt_b64enc_rem2");
    emitter.instruction("ldrb w12, [x1]");                                      // load byte 0
    emitter.instruction("ldrb w13, [x1, #1]");                                  // load byte 1
    // char 0: top 6 bits of byte0
    emitter.instruction("lsr w16, w12, #2");                                    // byte0 >> 2
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup
    emitter.instruction("strb w16, [x9], #1");                                  // write char 0
    // char 1: bottom 2 of byte0 + top 4 of byte1
    emitter.instruction("and w16, w12, #0x3");                                  // byte0 & 0x3
    emitter.instruction("lsl w16, w16, #4");                                    // shift left 4
    emitter.instruction("lsr w17, w13, #4");                                    // byte1 >> 4
    emitter.instruction("orr w16, w16, w17");                                   // combine
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup
    emitter.instruction("strb w16, [x9], #1");                                  // write char 1
    // char 2: bottom 4 of byte1 << 2
    emitter.instruction("and w16, w13, #0xf");                                  // byte1 & 0xf
    emitter.instruction("lsl w16, w16, #2");                                    // shift left 2
    emitter.instruction("ldrb w16, [x15, x16]");                                // lookup
    emitter.instruction("strb w16, [x9], #1");                                  // write char 2
    // padding
    emitter.instruction("mov w16, #61");                                        // '=' padding char
    emitter.instruction("strb w16, [x9], #1");                                  // write '='

    emitter.label("__rt_b64enc_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

fn emit_base64_encode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: base64_encode ---");
    emitter.label_global("__rt_base64_encode");

    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // load the current concat-buffer offset before appending the encoded bytes
    emitter.instruction("lea r9, [rip + _concat_buf]");                         // load the base address of the shared concat buffer
    emitter.instruction("add r9, r8");                                          // compute the destination pointer at the current concat-buffer tail
    emitter.instruction("mov r10, r9");                                         // preserve the encoded string start pointer for the return value
    emitter.instruction("mov rcx, rdx");                                        // copy the source byte count into a decrementing loop counter
    emitter.instruction("mov rsi, rax");                                        // copy the source pointer into a cursor register for byte-by-byte reads
    emitter.instruction("lea r11, [rip + _b64_encode_tbl]");                    // load the base64 lookup-table address for the encoding loop

    emitter.label("__rt_b64enc_loop_linux_x86_64");
    emitter.instruction("cmp rcx, 3");                                          // check whether at least one full 3-byte chunk remains
    emitter.instruction("jl __rt_b64enc_remainder_linux_x86_64");               // branch to the remainder path when fewer than 3 bytes remain

    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load source byte 0 and widen it for bit manipulation
    emitter.instruction("add rsi, 1");                                          // advance the source cursor past byte 0
    emitter.instruction("movzx edx, BYTE PTR [rsi]");                           // load source byte 1 and widen it for bit manipulation
    emitter.instruction("add rsi, 1");                                          // advance the source cursor past byte 1
    emitter.instruction("movzx r8d, BYTE PTR [rsi]");                           // load source byte 2 and widen it for bit manipulation
    emitter.instruction("add rsi, 1");                                          // advance the source cursor past byte 2
    emitter.instruction("sub rcx, 3");                                          // record that one full 3-byte chunk has been consumed

    emitter.instruction("mov edi, eax");                                        // copy byte 0 into a scratch register for char 0 encoding
    emitter.instruction("shr edi, 2");                                          // keep the top 6 bits of source byte 0
    emitter.instruction("movzx eax, BYTE PTR [r11 + rdi]");                     // map the 6-bit group through the base64 alphabet
    emitter.instruction("mov BYTE PTR [r9], al");                               // write encoded character 0 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing character 0

    emitter.instruction("mov edi, eax");                                        // reuse the integer scratch register while assembling encoded character 1
    emitter.instruction("mov edi, DWORD PTR [rsi - 3]");                        // reload source byte 0 from the just-consumed 3-byte chunk
    emitter.instruction("and edi, 3");                                          // keep the low 2 bits from source byte 0
    emitter.instruction("shl edi, 4");                                          // shift those 2 bits into the high half of the next 6-bit group
    emitter.instruction("mov eax, edx");                                        // copy source byte 1 into the integer scratch register for its upper nibble
    emitter.instruction("shr eax, 4");                                          // keep the top 4 bits from source byte 1
    emitter.instruction("or edi, eax");                                         // combine the carried byte-0 bits with the upper nibble from byte 1
    emitter.instruction("movzx eax, BYTE PTR [r11 + rdi]");                     // map the second 6-bit group through the base64 alphabet
    emitter.instruction("mov BYTE PTR [r9], al");                               // write encoded character 1 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing character 1

    emitter.instruction("mov edi, edx");                                        // seed the scratch register with source byte 1 while assembling encoded character 2
    emitter.instruction("and edi, 15");                                         // keep the low 4 bits from source byte 1
    emitter.instruction("shl edi, 2");                                          // shift those 4 bits into the high half of the next 6-bit group
    emitter.instruction("mov eax, r8d");                                        // copy source byte 2 into the integer scratch register for its upper bits
    emitter.instruction("shr eax, 6");                                          // keep the top 2 bits from source byte 2
    emitter.instruction("or edi, eax");                                         // combine the carried byte-1 bits with the upper bits from byte 2
    emitter.instruction("movzx eax, BYTE PTR [r11 + rdi]");                     // map the third 6-bit group through the base64 alphabet
    emitter.instruction("mov BYTE PTR [r9], al");                               // write encoded character 2 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing character 2

    emitter.instruction("mov edi, r8d");                                        // seed the scratch register with source byte 2 while assembling encoded character 3
    emitter.instruction("and edi, 63");                                         // keep the low 6 bits from source byte 2
    emitter.instruction("movzx eax, BYTE PTR [r11 + rdi]");                     // map the final 6-bit group through the base64 alphabet
    emitter.instruction("mov BYTE PTR [r9], al");                               // write encoded character 3 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing character 3
    emitter.instruction("jmp __rt_b64enc_loop_linux_x86_64");                   // continue encoding subsequent 3-byte chunks

    emitter.label("__rt_b64enc_remainder_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once no remainder bytes remain after the main loop
    emitter.instruction("je __rt_b64enc_done_linux_x86_64");                    // skip the remainder path when the input length was an exact multiple of 3
    emitter.instruction("cmp rcx, 1");                                          // check whether exactly one source byte remains
    emitter.instruction("jne __rt_b64enc_rem2_linux_x86_64");                   // branch to the two-byte remainder path when two bytes remain

    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the final source byte for the 1-byte remainder case
    emitter.instruction("mov edi, eax");                                        // copy the remaining byte into a scratch register for char 0 encoding
    emitter.instruction("shr edi, 2");                                          // keep the top 6 bits from the remaining source byte
    emitter.instruction("movzx eax, BYTE PTR [r11 + rdi]");                     // map the first remainder group through the base64 alphabet
    emitter.instruction("mov BYTE PTR [r9], al");                               // write encoded remainder character 0 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing character 0
    emitter.instruction("movzx edi, BYTE PTR [rsi]");                           // reload the remaining source byte while assembling encoded remainder character 1
    emitter.instruction("and edi, 3");                                          // keep the low 2 bits from the remaining source byte
    emitter.instruction("shl edi, 4");                                          // shift those 2 bits into the high half of the next 6-bit group
    emitter.instruction("movzx eax, BYTE PTR [r11 + rdi]");                     // map the second remainder group through the base64 alphabet
    emitter.instruction("mov BYTE PTR [r9], al");                               // write encoded remainder character 1 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing character 1
    emitter.instruction("mov BYTE PTR [r9], 61");                               // append the first '=' padding byte for the 1-byte remainder case
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after the first padding byte
    emitter.instruction("mov BYTE PTR [r9], 61");                               // append the second '=' padding byte for the 1-byte remainder case
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after the second padding byte
    emitter.instruction("jmp __rt_b64enc_done_linux_x86_64");                   // finish after handling the 1-byte remainder case

    emitter.label("__rt_b64enc_rem2_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load remainder source byte 0 for the 2-byte remainder case
    emitter.instruction("movzx edx, BYTE PTR [rsi + 1]");                       // load remainder source byte 1 for the 2-byte remainder case
    emitter.instruction("mov edi, eax");                                        // copy remainder byte 0 into a scratch register for char 0 encoding
    emitter.instruction("shr edi, 2");                                          // keep the top 6 bits from remainder byte 0
    emitter.instruction("movzx eax, BYTE PTR [r11 + rdi]");                     // map the first remainder group through the base64 alphabet
    emitter.instruction("mov BYTE PTR [r9], al");                               // write encoded remainder character 0 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing character 0
    emitter.instruction("movzx edi, BYTE PTR [rsi]");                           // reload remainder byte 0 while assembling encoded remainder character 1
    emitter.instruction("and edi, 3");                                          // keep the low 2 bits from remainder byte 0
    emitter.instruction("shl edi, 4");                                          // shift those 2 bits into the high half of the next 6-bit group
    emitter.instruction("mov eax, edx");                                        // copy remainder byte 1 into the integer scratch register for its upper nibble
    emitter.instruction("shr eax, 4");                                          // keep the top 4 bits from remainder byte 1
    emitter.instruction("or edi, eax");                                         // combine the carried byte-0 bits with the upper nibble from remainder byte 1
    emitter.instruction("movzx eax, BYTE PTR [r11 + rdi]");                     // map the second remainder group through the base64 alphabet
    emitter.instruction("mov BYTE PTR [r9], al");                               // write encoded remainder character 1 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing character 1
    emitter.instruction("mov edi, edx");                                        // seed the scratch register with remainder byte 1 while assembling encoded remainder character 2
    emitter.instruction("and edi, 15");                                         // keep the low 4 bits from remainder byte 1
    emitter.instruction("shl edi, 2");                                          // shift those 4 bits into the high half of the next 6-bit group
    emitter.instruction("movzx eax, BYTE PTR [r11 + rdi]");                     // map the third remainder group through the base64 alphabet
    emitter.instruction("mov BYTE PTR [r9], al");                               // write encoded remainder character 2 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing character 2
    emitter.instruction("mov BYTE PTR [r9], 61");                               // append the single '=' padding byte for the 2-byte remainder case
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after the padding byte

    emitter.label("__rt_b64enc_done_linux_x86_64");
    emitter.instruction("mov rax, r10");                                        // return the encoded string start pointer in the standard x86_64 string result register
    emitter.instruction("mov rdx, r9");                                         // copy the concat-buffer tail into the length scratch register
    emitter.instruction("sub rdx, r10");                                        // compute the encoded string length from the written byte count
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r9");               // temporarily publish the absolute concat-buffer tail before normalizing the shared offset
    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // reload the absolute concat-buffer tail through the shared offset slot
    emitter.instruction("lea r9, [rip + _concat_buf]");                         // load the concat-buffer base so the shared offset can stay relative
    emitter.instruction("sub r8, r9");                                          // convert the absolute concat-buffer tail back into the shared relative offset
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r8");               // publish the updated relative concat-buffer offset for later string appenders
    emitter.instruction("ret");                                                 // return the encoded string through the standard x86_64 string result registers
}
