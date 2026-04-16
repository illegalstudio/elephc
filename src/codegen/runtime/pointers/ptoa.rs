use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// __rt_ptoa: convert pointer address to hex string "0x...".
/// Input:  x0 = pointer value (64-bit address)
/// Output: x1 = string pointer (in concat_buf), x2 = string length
pub(crate) fn emit_ptoa(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ptoa_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for ARM64 instructions
    emitter.comment("--- runtime: ptoa (pointer to hex string) ---");
    emitter.label_global("__rt_ptoa");

    // -- save return address --
    emitter.instruction("str x30, [sp, #-16]!");                                // save link register

    // -- set up output buffer in concat_buf --
    emitter.adrp("x1", "_concat_buf");                           // load page of concat buffer
    emitter.add_lo12("x1", "x1", "_concat_buf");                     // resolve concat buffer address
    emitter.instruction("mov x3, x1");                                          // x3 = write cursor

    // -- write "0x" prefix --
    emitter.instruction("mov w4, #0x30");                                       // ASCII '0'
    emitter.instruction("strb w4, [x3], #1");                                   // write '0', advance cursor
    emitter.instruction("mov w4, #0x78");                                       // ASCII 'x'
    emitter.instruction("strb w4, [x3], #1");                                   // write 'x', advance cursor

    // -- handle zero specially --
    emitter.instruction("cbnz x0, __rt_ptoa_find_start");                       // non-zero, find first nibble
    emitter.instruction("mov w4, #0x30");                                       // ASCII '0'
    emitter.instruction("strb w4, [x3], #1");                                   // write single '0' for null pointer
    emitter.instruction("b __rt_ptoa_done");                                    // skip to end

    // -- find first non-zero nibble (skip leading zeros) --
    emitter.label("__rt_ptoa_find_start");
    emitter.instruction("clz x5, x0");                                          // count leading zero bits
    emitter.instruction("lsr x5, x5, #2");                                      // divide by 4 = leading zero nibbles
    emitter.instruction("mov x6, #16");                                         // total nibbles in 64-bit value
    emitter.instruction("sub x6, x6, x5");                                      // x6 = significant nibbles to emit
    emitter.instruction("lsl x5, x5, #2");                                      // x5 = bits to shift left to align first nibble
    emitter.instruction("lsl x0, x0, x5");                                      // shift value so first significant nibble is at top

    // -- emit hex digits loop --
    emitter.label("__rt_ptoa_loop");
    emitter.instruction("cbz x6, __rt_ptoa_done");                              // all nibbles emitted
    emitter.instruction("lsr x4, x0, #60");                                     // extract top 4 bits (current nibble)
    emitter.instruction("cmp x4, #10");                                         // is it >= 10?
    emitter.instruction("b.ge __rt_ptoa_hex_letter");                           // yes, use a-f
    emitter.instruction("add x4, x4, #0x30");                                   // convert 0-9 to ASCII '0'-'9'
    emitter.instruction("b __rt_ptoa_store");                                   // go store the digit

    emitter.label("__rt_ptoa_hex_letter");
    emitter.instruction("add x4, x4, #0x57");                                   // convert 10-15 to ASCII 'a'-'f' (10+0x57=0x61='a')

    emitter.label("__rt_ptoa_store");
    emitter.instruction("strb w4, [x3], #1");                                   // store hex digit, advance cursor
    emitter.instruction("lsl x0, x0, #4");                                      // shift next nibble into top position
    emitter.instruction("sub x6, x6, #1");                                      // decrement remaining nibble count
    emitter.instruction("b __rt_ptoa_loop");                                    // continue loop

    // -- compute length and return --
    emitter.label("__rt_ptoa_done");
    emitter.instruction("sub x2, x3, x1");                                      // x2 = length (cursor - start)
    emitter.instruction("ldr x30, [sp], #16");                                  // restore link register
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_ptoa_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ptoa (pointer to hex string) ---");
    emitter.label_global("__rt_ptoa");

    emitter.instruction("mov r11, rax");                                        // preserve the incoming pointer payload because x86_64 call sites pass it in the integer result register
    abi::emit_symbol_address(emitter, "rax", "_concat_buf");
    emitter.instruction("mov rsi, rax");                                        // seed the write cursor from the concat buffer base so the helper can emit the pointer string in-place
    emitter.instruction("mov BYTE PTR [rsi], 0x30");                            // write the leading '0' of the hexadecimal pointer prefix
    emitter.instruction("add rsi, 1");                                          // advance the write cursor after storing the leading '0'
    emitter.instruction("mov BYTE PTR [rsi], 0x78");                            // write the trailing 'x' of the hexadecimal pointer prefix
    emitter.instruction("add rsi, 1");                                          // advance the write cursor after storing the trailing 'x'
    emitter.instruction("test r11, r11");                                       // detect the null pointer case before scanning for the first significant nibble
    emitter.instruction("jnz __rt_ptoa_find_start_linux_x86_64");               // skip the null fast path when the incoming pointer value is non-zero
    emitter.instruction("mov BYTE PTR [rsi], 0x30");                            // encode a single trailing '0' digit when the incoming pointer value is null
    emitter.instruction("add rsi, 1");                                          // advance the write cursor after materializing the null pointer payload digit
    emitter.instruction("jmp __rt_ptoa_done_linux_x86_64");                     // finish once the hexadecimal null pointer spelling has been emitted

    emitter.label("__rt_ptoa_find_start_linux_x86_64");
    emitter.instruction("bsr r8, r11");                                         // locate the highest set bit so the helper can skip leading zero nibbles
    emitter.instruction("mov r9, r8");                                          // copy the highest-set-bit index before converting it into a nibble count
    emitter.instruction("shr r9, 2");                                           // divide the highest-set-bit index by four to obtain the most significant nibble index
    emitter.instruction("add r9, 1");                                           // convert the nibble index into the total number of significant hexadecimal digits
    emitter.instruction("mov r10, 60");                                         // seed the top-nibble shift distance used to peel hexadecimal digits from the pointer value
    emitter.instruction("sub r10, r8");                                         // compute how many high-order bits precede the first significant bit
    emitter.instruction("and r10, -4");                                         // round the leading-bit distance down to a nibble boundary for hexadecimal emission
    emitter.instruction("mov rdx, r11");                                        // copy the incoming pointer value into a scratch register that can be shifted during digit emission
    emitter.instruction("mov cl, r10b");                                        // move the initial nibble-alignment shift into the x86 variable-shift register
    emitter.instruction("shl rdx, cl");                                         // align the first significant nibble at the top of the working pointer value

    emitter.label("__rt_ptoa_loop_linux_x86_64");
    emitter.instruction("test r9, r9");                                         // stop once every significant hexadecimal digit has been emitted
    emitter.instruction("jz __rt_ptoa_done_linux_x86_64");                      // finish once no significant hexadecimal digits remain
    emitter.instruction("mov rcx, rdx");                                        // copy the shifted working pointer value before extracting the current high nibble
    emitter.instruction("shr rcx, 60");                                         // isolate the current hexadecimal digit in the low bits of the scratch register
    emitter.instruction("cmp rcx, 10");                                         // decide whether the current nibble maps to '0'-'9' or 'a'-'f'
    emitter.instruction("jge __rt_ptoa_hex_letter_linux_x86_64");               // branch to the alphabetic hexadecimal digit path for nibble values ten through fifteen
    emitter.instruction("add ecx, 0x30");                                       // convert nibble values zero through nine into ASCII '0' through '9'
    emitter.instruction("jmp __rt_ptoa_store_linux_x86_64");                    // skip the alphabetic adjustment once the numeric digit has been materialized

    emitter.label("__rt_ptoa_hex_letter_linux_x86_64");
    emitter.instruction("add ecx, 0x57");                                       // convert nibble values ten through fifteen into ASCII 'a' through 'f'

    emitter.label("__rt_ptoa_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [rsi], cl");                              // store the current hexadecimal digit into the concat buffer
    emitter.instruction("add rsi, 1");                                          // advance the write cursor after storing one hexadecimal digit
    emitter.instruction("shl rdx, 4");                                          // shift the next nibble into the high position for the following loop iteration
    emitter.instruction("sub r9, 1");                                           // decrement the count of significant hexadecimal digits still left to emit
    emitter.instruction("jmp __rt_ptoa_loop_linux_x86_64");                     // continue emitting the remaining hexadecimal digits

    emitter.label("__rt_ptoa_done_linux_x86_64");
    emitter.instruction("mov rdx, rsi");                                        // copy the final write cursor before converting it into the emitted string length
    emitter.instruction("sub rdx, rax");                                        // compute the emitted string length as write cursor minus concat buffer start
    emitter.instruction("ret");                                                 // return the concat buffer pointer and emitted length in the x86_64 string result registers
}
