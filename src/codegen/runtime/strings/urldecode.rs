use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// urldecode: decode %XX hex sequences and '+' to space.
/// Input: x1/x2=string. Output: x1/x2=result in concat_buf.
pub fn emit_urldecode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_urldecode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: urldecode ---");
    emitter.label_global("__rt_urldecode");

    // -- set up concat_buf destination --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_urldecode_loop");
    emitter.instruction("cbz x11, __rt_urldecode_done");                        // no bytes left -> done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load source byte, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining

    // -- check '+' -> space --
    emitter.instruction("cmp w12, #43");                                        // is it '+'?
    emitter.instruction("b.ne __rt_urldecode_chk_pct");                         // no -> check '%'
    emitter.instruction("mov w13, #32");                                        // space character
    emitter.instruction("strb w13, [x9], #1");                                  // write space
    emitter.instruction("b __rt_urldecode_loop");                               // next byte

    // -- check '%' -> decode hex pair --
    emitter.label("__rt_urldecode_chk_pct");
    emitter.instruction("cmp w12, #37");                                        // is it '%'?
    emitter.instruction("b.ne __rt_urldecode_store");                           // no -> store as-is
    emitter.instruction("cmp x11, #2");                                         // need at least 2 more bytes
    emitter.instruction("b.lt __rt_urldecode_store_pct");                       // not enough -> store '%'

    // -- decode high nibble --
    emitter.instruction("ldrb w12, [x1], #1");                                  // load first hex char
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le __rt_urldecode_hi_num");                          // yes -> numeric
    emitter.instruction("cmp w12, #70");                                        // <= 'F'?
    emitter.instruction("b.le __rt_urldecode_hi_uc");                           // yes -> uppercase
    emitter.instruction("sub w12, w12, #87");                                   // 'a'-'f' -> 10-15
    emitter.instruction("b __rt_urldecode_hi_done");                            // done with high nibble
    emitter.label("__rt_urldecode_hi_num");
    emitter.instruction("sub w12, w12, #48");                                   // '0'-'9' -> 0-9
    emitter.instruction("b __rt_urldecode_hi_done");                            // done
    emitter.label("__rt_urldecode_hi_uc");
    emitter.instruction("sub w12, w12, #55");                                   // 'A'-'F' -> 10-15
    emitter.label("__rt_urldecode_hi_done");
    emitter.instruction("lsl w13, w12, #4");                                    // shift to high nibble

    // -- decode low nibble --
    emitter.instruction("ldrb w12, [x1], #1");                                  // load second hex char
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le __rt_urldecode_lo_num");                          // yes -> numeric
    emitter.instruction("cmp w12, #70");                                        // <= 'F'?
    emitter.instruction("b.le __rt_urldecode_lo_uc");                           // yes -> uppercase
    emitter.instruction("sub w12, w12, #87");                                   // 'a'-'f' -> 10-15
    emitter.instruction("b __rt_urldecode_lo_done");                            // done with low nibble
    emitter.label("__rt_urldecode_lo_num");
    emitter.instruction("sub w12, w12, #48");                                   // '0'-'9' -> 0-9
    emitter.instruction("b __rt_urldecode_lo_done");                            // done
    emitter.label("__rt_urldecode_lo_uc");
    emitter.instruction("sub w12, w12, #55");                                   // 'A'-'F' -> 10-15
    emitter.label("__rt_urldecode_lo_done");
    emitter.instruction("orr w13, w13, w12");                                   // combine high and low nibbles
    emitter.instruction("strb w13, [x9], #1");                                  // store decoded byte
    emitter.instruction("b __rt_urldecode_loop");                               // next iteration

    // -- store '%' as-is (not enough chars for hex pair) --
    emitter.label("__rt_urldecode_store_pct");
    emitter.instruction("mov w12, #37");                                        // '%' character
    emitter.label("__rt_urldecode_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store byte as-is
    emitter.instruction("b __rt_urldecode_loop");                               // next byte

    emitter.label("__rt_urldecode_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

fn emit_urldecode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: urldecode ---");
    emitter.label_global("__rt_urldecode");

    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before decoding query-style percent-encoded bytes
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the concat-buffer destination pointer where the decoded string begins
    emitter.instruction("mov r8, r11");                                         // preserve the concat-backed result start pointer for the returned string value after the loop mutates the destination cursor
    emitter.instruction("mov rcx, rdx");                                        // seed the remaining source length counter from the borrowed percent-encoded input string length
    emitter.instruction("mov rsi, rax");                                        // preserve the borrowed source string cursor in a dedicated register before the loop mutates caller-saved registers

    emitter.label("__rt_urldecode_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been classified and copied or decoded into concat storage
    emitter.instruction("jz __rt_urldecode_done_linux_x86_64");                 // finish once the full borrowed source string has been consumed
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load one source byte before deciding whether urldecode() must translate it
    emitter.instruction("add rsi, 1");                                          // advance the borrowed source string cursor after consuming one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining source length after consuming one byte
    emitter.instruction("cmp dl, 43");                                          // is the current source byte '+' which query-style urldecode() maps back to a space?
    emitter.instruction("jne __rt_urldecode_chk_pct_linux_x86_64");             // continue with the percent-sequence probe when the current byte is not '+'
    emitter.instruction("mov BYTE PTR [r11], 32");                              // write a literal space because query-style urldecode() maps '+' back to ASCII space
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting one decoded space
    emitter.instruction("jmp __rt_urldecode_loop_linux_x86_64");                // continue decoding the remainder of the source string after handling one plus sign

    emitter.label("__rt_urldecode_chk_pct_linux_x86_64");
    emitter.instruction("cmp dl, 37");                                          // is the current source byte '%' which may begin a two-digit hexadecimal escape?
    emitter.instruction("jne __rt_urldecode_store_linux_x86_64");               // copy bytes that do not begin with '%' straight through without percent decoding
    emitter.instruction("cmp rcx, 2");                                          // are there at least two more source bytes available for a complete `%XX` escape?
    emitter.instruction("jb __rt_urldecode_store_pct_linux_x86_64");            // copy a trailing '%' literally when the escape sequence is incomplete
    emitter.instruction("movzx r10d, BYTE PTR [rsi]");                          // load the high hexadecimal digit of the `%XX` escape before decoding it into the output byte
    emitter.instruction("add rsi, 1");                                          // advance the borrowed source string cursor after consuming the high hexadecimal digit
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining source length after consuming the high hexadecimal digit
    emitter.instruction("cmp r10b, 57");                                        // is the high hexadecimal digit within '0'..'9' so it can decode numerically?
    emitter.instruction("jbe __rt_urldecode_hi_num_linux_x86_64");              // decode decimal high digits through the numeric branch
    emitter.instruction("cmp r10b, 70");                                        // is the high hexadecimal digit within 'A'..'F' so it can decode as uppercase hex?
    emitter.instruction("jbe __rt_urldecode_hi_uc_linux_x86_64");               // decode uppercase hexadecimal high digits through the uppercase branch
    emitter.instruction("sub r10b, 87");                                        // decode lowercase hexadecimal high digits by mapping 'a'..'f' to 10..15
    emitter.instruction("jmp __rt_urldecode_hi_done_linux_x86_64");             // continue once the high hexadecimal nibble has been decoded

    emitter.label("__rt_urldecode_hi_num_linux_x86_64");
    emitter.instruction("sub r10b, 48");                                        // decode numeric high digits by mapping '0'..'9' to 0..9
    emitter.instruction("jmp __rt_urldecode_hi_done_linux_x86_64");             // continue once the numeric high hexadecimal nibble has been decoded

    emitter.label("__rt_urldecode_hi_uc_linux_x86_64");
    emitter.instruction("sub r10b, 55");                                        // decode uppercase hexadecimal high digits by mapping 'A'..'F' to 10..15

    emitter.label("__rt_urldecode_hi_done_linux_x86_64");
    emitter.instruction("shl r10b, 4");                                         // move the decoded high hexadecimal nibble into the upper half of the output byte
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the low hexadecimal digit of the `%XX` escape before decoding it into the output byte
    emitter.instruction("add rsi, 1");                                          // advance the borrowed source string cursor after consuming the low hexadecimal digit
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining source length after consuming the low hexadecimal digit
    emitter.instruction("cmp al, 57");                                          // is the low hexadecimal digit within '0'..'9' so it can decode numerically?
    emitter.instruction("jbe __rt_urldecode_lo_num_linux_x86_64");              // decode decimal low digits through the numeric branch
    emitter.instruction("cmp al, 70");                                          // is the low hexadecimal digit within 'A'..'F' so it can decode as uppercase hex?
    emitter.instruction("jbe __rt_urldecode_lo_uc_linux_x86_64");               // decode uppercase hexadecimal low digits through the uppercase branch
    emitter.instruction("sub al, 87");                                          // decode lowercase hexadecimal low digits by mapping 'a'..'f' to 10..15
    emitter.instruction("jmp __rt_urldecode_lo_done_linux_x86_64");             // continue once the low hexadecimal nibble has been decoded

    emitter.label("__rt_urldecode_lo_num_linux_x86_64");
    emitter.instruction("sub al, 48");                                          // decode numeric low digits by mapping '0'..'9' to 0..9
    emitter.instruction("jmp __rt_urldecode_lo_done_linux_x86_64");             // continue once the numeric low hexadecimal nibble has been decoded

    emitter.label("__rt_urldecode_lo_uc_linux_x86_64");
    emitter.instruction("sub al, 55");                                          // decode uppercase hexadecimal low digits by mapping 'A'..'F' to 10..15

    emitter.label("__rt_urldecode_lo_done_linux_x86_64");
    emitter.instruction("or al, r10b");                                         // merge the decoded high and low hexadecimal nibbles into the single output byte
    emitter.instruction("mov BYTE PTR [r11], al");                              // store the decoded output byte into concat storage after parsing one complete `%XX` escape
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting one decoded output byte
    emitter.instruction("jmp __rt_urldecode_loop_linux_x86_64");                // continue decoding the remaining source bytes after one successful `%XX` expansion

    emitter.label("__rt_urldecode_store_pct_linux_x86_64");
    emitter.instruction("mov dl, 37");                                          // restore a literal '%' when the percent escape is truncated and cannot be decoded

    emitter.label("__rt_urldecode_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], dl");                              // copy undecoded source bytes straight through into concat storage when they are not valid query escapes
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after copying one undecoded source byte
    emitter.instruction("jmp __rt_urldecode_loop_linux_x86_64");                // continue decoding the remaining source bytes after one literal-byte copy

    emitter.label("__rt_urldecode_done_linux_x86_64");
    emitter.instruction("mov rax, r8");                                         // return the concat-backed result start pointer after decoding the full input string
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer destination cursor before computing the decoded string length
    emitter.instruction("sub rdx, r8");                                         // compute the decoded string length as dest_end - dest_start for the returned x86_64 string value
    emitter.instruction("mov rcx, QWORD PTR [rip + _concat_off]");              // reload the concat-buffer write offset before publishing the bytes that urldecode() appended
    emitter.instruction("add rcx, rdx");                                        // advance the concat-buffer write offset by the produced decoded-string length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // persist the updated concat-buffer write offset after finishing the urldecode() pass
    emitter.instruction("ret");                                                 // return the concat-backed decoded string in the standard x86_64 string result registers
}
