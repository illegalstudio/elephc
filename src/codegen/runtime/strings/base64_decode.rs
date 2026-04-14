use crate::codegen::{emit::Emitter, platform::Arch};

/// base64_decode: standard base64 decoding (4 input chars -> 3 output bytes).
/// Input: x1/x2=string. Output: x1/x2=result in concat_buf.
/// Uses _b64_decode_tbl data section for the reverse lookup table.
pub fn emit_base64_decode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_base64_decode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: base64_decode ---");
    emitter.label_global("__rt_base64_decode");

    // -- set up concat_buf destination --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    // -- load base64 decode lookup table --
    crate::codegen::abi::emit_symbol_address(emitter, "x15", "_b64_decode_tbl");

    // -- process 4 chars at a time --
    emitter.label("__rt_b64dec_loop");
    emitter.instruction("cmp x11, #4");                                         // at least 4 chars left?
    emitter.instruction("b.lt __rt_b64dec_done");                               // no -> done

    // -- load and decode 4 base64 chars --
    emitter.instruction("ldrb w12, [x1], #1");                                  // load char 0
    emitter.instruction("ldrb w12, [x15, x12]");                                // decode char 0 via table
    emitter.instruction("ldrb w13, [x1], #1");                                  // load char 1
    emitter.instruction("ldrb w13, [x15, x13]");                                // decode char 1 via table
    emitter.instruction("ldrb w14, [x1], #1");                                  // load char 2
    emitter.instruction("ldrb w16, [x1], #1");                                  // load char 3
    emitter.instruction("sub x11, x11, #4");                                    // consumed 4 chars

    // -- check for '=' padding in char 2 --
    emitter.instruction("cmp w14, #61");                                        // is char 2 '='?
    emitter.instruction("b.eq __rt_b64dec_pad2");                               // yes -> only 1 output byte

    // -- decode char 2 via table --
    emitter.instruction("ldrb w14, [x15, x14]");                                // decode char 2

    // -- check for '=' padding in char 3 --
    emitter.instruction("cmp w16, #61");                                        // is char 3 '='?
    emitter.instruction("b.eq __rt_b64dec_pad1");                               // yes -> only 2 output bytes

    // -- decode char 3 via table --
    emitter.instruction("ldrb w16, [x15, x16]");                                // decode char 3

    // -- output byte 0: (val0 << 2) | (val1 >> 4) --
    emitter.instruction("lsl w17, w12, #2");                                    // val0 << 2
    emitter.instruction("lsr w18, w13, #4");                                    // val1 >> 4
    emitter.instruction("orr w17, w17, w18");                                   // combine
    emitter.instruction("strb w17, [x9], #1");                                  // write byte 0

    // -- output byte 1: (val1 << 4) | (val2 >> 2) --
    emitter.instruction("and w17, w13, #0xf");                                  // val1 & 0xf
    emitter.instruction("lsl w17, w17, #4");                                    // shift left 4
    emitter.instruction("lsr w18, w14, #2");                                    // val2 >> 2
    emitter.instruction("orr w17, w17, w18");                                   // combine
    emitter.instruction("strb w17, [x9], #1");                                  // write byte 1

    // -- output byte 2: (val2 << 6) | val3 --
    emitter.instruction("and w17, w14, #0x3");                                  // val2 & 0x3
    emitter.instruction("lsl w17, w17, #6");                                    // shift left 6
    emitter.instruction("orr w17, w17, w16");                                   // combine with val3
    emitter.instruction("strb w17, [x9], #1");                                  // write byte 2
    emitter.instruction("b __rt_b64dec_loop");                                  // next 4 chars

    // -- padding: char2 is '=', only 1 output byte --
    emitter.label("__rt_b64dec_pad2");
    emitter.instruction("lsl w17, w12, #2");                                    // val0 << 2
    emitter.instruction("lsr w18, w13, #4");                                    // val1 >> 4
    emitter.instruction("orr w17, w17, w18");                                   // combine
    emitter.instruction("strb w17, [x9], #1");                                  // write byte 0
    emitter.instruction("b __rt_b64dec_done");                                  // done (skip rest)

    // -- padding: char3 is '=', only 2 output bytes --
    emitter.label("__rt_b64dec_pad1");
    // output byte 0
    emitter.instruction("lsl w17, w12, #2");                                    // val0 << 2
    emitter.instruction("lsr w18, w13, #4");                                    // val1 >> 4
    emitter.instruction("orr w17, w17, w18");                                   // combine
    emitter.instruction("strb w17, [x9], #1");                                  // write byte 0
    // output byte 1
    emitter.instruction("and w17, w13, #0xf");                                  // val1 & 0xf
    emitter.instruction("lsl w17, w17, #4");                                    // shift left 4
    emitter.instruction("lsr w18, w14, #2");                                    // val2 >> 2
    emitter.instruction("orr w17, w17, w18");                                   // combine
    emitter.instruction("strb w17, [x9], #1");                                  // write byte 1
    emitter.instruction("b __rt_b64dec_done");                                  // done (skip rest)

    emitter.label("__rt_b64dec_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

fn emit_base64_decode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: base64_decode ---");
    emitter.label_global("__rt_base64_decode");

    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // load the current concat-buffer offset before appending the decoded bytes
    emitter.instruction("lea r9, [rip + _concat_buf]");                         // load the base address of the shared concat buffer
    emitter.instruction("add r9, r8");                                          // compute the destination pointer at the current concat-buffer tail
    emitter.instruction("mov r10, r9");                                         // preserve the decoded string start pointer for the return value
    emitter.instruction("mov rcx, rdx");                                        // copy the encoded character count into a decrementing loop counter
    emitter.instruction("mov rsi, rax");                                        // copy the encoded string pointer into a cursor register for byte-by-byte reads
    emitter.label("__rt_b64dec_loop_linux_x86_64");
    emitter.instruction("lea r11, [rip + _b64_decode_tbl]");                    // reload the base64 reverse-lookup table address for this decoding iteration
    emitter.instruction("cmp rcx, 4");                                          // check whether at least one full 4-character chunk remains
    emitter.instruction("jl __rt_b64dec_done_linux_x86_64");                    // stop once fewer than 4 encoded characters remain

    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load encoded char 0 and widen it for table lookup
    emitter.instruction("add rsi, 1");                                          // advance the encoded-string cursor past char 0
    emitter.instruction("movzx eax, BYTE PTR [r11 + rax]");                     // decode char 0 through the reverse lookup table
    emitter.instruction("movzx edx, BYTE PTR [rsi]");                           // load encoded char 1 and widen it for table lookup
    emitter.instruction("add rsi, 1");                                          // advance the encoded-string cursor past char 1
    emitter.instruction("movzx edx, BYTE PTR [r11 + rdx]");                     // decode char 1 through the reverse lookup table
    emitter.instruction("movzx r8d, BYTE PTR [rsi]");                           // load encoded char 2 so padding can be checked before table lookup
    emitter.instruction("add rsi, 1");                                          // advance the encoded-string cursor past char 2
    emitter.instruction("movzx edi, BYTE PTR [rsi]");                           // load encoded char 3 so padding can be checked before table lookup
    emitter.instruction("add rsi, 1");                                          // advance the encoded-string cursor past char 3
    emitter.instruction("sub rcx, 4");                                          // record that one full 4-character chunk has been consumed

    emitter.instruction("cmp r8d, 61");                                         // check whether encoded char 2 is '=' padding
    emitter.instruction("je __rt_b64dec_pad2_linux_x86_64");                    // branch when only one decoded output byte remains
    emitter.instruction("movzx r8d, BYTE PTR [r11 + r8]");                      // decode char 2 through the reverse lookup table
    emitter.instruction("cmp edi, 61");                                         // check whether encoded char 3 is '=' padding
    emitter.instruction("je __rt_b64dec_pad1_linux_x86_64");                    // branch when only two decoded output bytes remain
    emitter.instruction("movzx edi, BYTE PTR [r11 + rdi]");                     // decode char 3 through the reverse lookup table

    emitter.instruction("shl eax, 2");                                          // move decoded value 0 into the output-byte 0 position
    emitter.instruction("mov r11d, edx");                                       // copy decoded value 1 into a scratch register for output byte 0 assembly
    emitter.instruction("shr r11d, 4");                                         // keep the upper 2 decoded bits from value 1 for output byte 0
    emitter.instruction("or eax, r11d");                                        // combine the carried decoded bits into output byte 0
    emitter.instruction("mov BYTE PTR [r9], al");                               // write decoded output byte 0 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing output byte 0

    emitter.instruction("and edx, 15");                                         // keep the low 4 decoded bits from value 1 for output byte 1
    emitter.instruction("shl edx, 4");                                          // move those 4 bits into their output-byte position
    emitter.instruction("mov r11d, r8d");                                       // copy decoded value 2 into a scratch register for its upper bits
    emitter.instruction("shr r11d, 2");                                         // keep the upper 4 decoded bits from value 2 for output byte 1
    emitter.instruction("or edx, r11d");                                        // combine the carried decoded bits into output byte 1
    emitter.instruction("mov BYTE PTR [r9], dl");                               // write decoded output byte 1 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing output byte 1

    emitter.instruction("and r8d, 3");                                          // keep the low 2 decoded bits from value 2 for output byte 2
    emitter.instruction("shl r8d, 6");                                          // move those 2 bits into their output-byte position
    emitter.instruction("or r8d, edi");                                         // combine the carried decoded bits with decoded value 3
    emitter.instruction("mov BYTE PTR [r9], r8b");                              // write decoded output byte 2 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing output byte 2
    emitter.instruction("jmp __rt_b64dec_loop_linux_x86_64");                   // continue decoding subsequent 4-character chunks

    emitter.label("__rt_b64dec_pad2_linux_x86_64");
    emitter.instruction("shl eax, 2");                                          // move decoded value 0 into the output-byte position for the '==' padded chunk
    emitter.instruction("mov r11d, edx");                                       // copy decoded value 1 into a scratch register for the padded output byte
    emitter.instruction("shr r11d, 4");                                         // keep the upper 2 decoded bits from value 1 for the padded output byte
    emitter.instruction("or eax, r11d");                                        // combine the carried decoded bits into the single padded output byte
    emitter.instruction("mov BYTE PTR [r9], al");                               // write the single decoded output byte for the '==' padded chunk
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing the single padded output byte
    emitter.instruction("jmp __rt_b64dec_done_linux_x86_64");                   // finish after the '==' padded chunk

    emitter.label("__rt_b64dec_pad1_linux_x86_64");
    emitter.instruction("shl eax, 2");                                          // move decoded value 0 into the output-byte 0 position for the '=' padded chunk
    emitter.instruction("mov r11d, edx");                                       // copy decoded value 1 into a scratch register for output byte 0 assembly
    emitter.instruction("shr r11d, 4");                                         // keep the upper 2 decoded bits from value 1 for output byte 0
    emitter.instruction("or eax, r11d");                                        // combine the carried decoded bits into output byte 0
    emitter.instruction("mov BYTE PTR [r9], al");                               // write decoded output byte 0 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing output byte 0
    emitter.instruction("and edx, 15");                                         // keep the low 4 decoded bits from value 1 for output byte 1
    emitter.instruction("shl edx, 4");                                          // move those 4 bits into their output-byte position
    emitter.instruction("mov r11d, r8d");                                       // copy decoded value 2 into a scratch register for its upper bits
    emitter.instruction("shr r11d, 2");                                         // keep the upper 4 decoded bits from value 2 for output byte 1
    emitter.instruction("or edx, r11d");                                        // combine the carried decoded bits into output byte 1
    emitter.instruction("mov BYTE PTR [r9], dl");                               // write decoded output byte 1 to the destination buffer
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after writing output byte 1

    emitter.label("__rt_b64dec_done_linux_x86_64");
    emitter.instruction("mov rax, r10");                                        // return the decoded string start pointer in the standard x86_64 string result register
    emitter.instruction("mov rdx, r9");                                         // copy the concat-buffer tail into the length scratch register
    emitter.instruction("sub rdx, r10");                                        // compute the decoded string length from the written byte count
    emitter.instruction("mov r8, r9");                                          // copy the absolute concat-buffer tail before normalizing it back to a shared offset
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // load the concat-buffer base so the shared offset can stay relative
    emitter.instruction("sub r8, r11");                                         // convert the absolute concat-buffer tail back into the shared relative offset
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r8");               // publish the updated relative concat-buffer offset for later string appenders
    emitter.instruction("ret");                                                 // return the decoded string through the standard x86_64 string result registers
}
