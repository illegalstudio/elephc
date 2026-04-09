use crate::codegen::{emit::Emitter, platform::Arch};

/// itoa: convert signed 64-bit integer to decimal string.
/// Input:  x0 = integer value
/// Output: x1 = pointer to string, x2 = length
pub fn emit_itoa(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_itoa_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: itoa ---");
    emitter.label_global("__rt_itoa");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset into concat_buf
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute write position: buf + offset
    emitter.instruction("add x9, x9, #20");                                     // advance to end of 21-byte scratch area (digits written right-to-left)

    // -- initialize counters --
    emitter.instruction("mov x10, #0");                                         // digit count = 0
    emitter.instruction("mov x11, #0");                                         // negative flag = 0 (not negative)

    // -- handle sign --
    emitter.instruction("cmp x0, #0");                                          // check if input is negative
    emitter.instruction("b.ge __rt_itoa_positive");                             // skip negation if >= 0
    emitter.instruction("mov x11, #1");                                         // set negative flag
    emitter.instruction("neg x0, x0");                                          // negate to make value positive

    // -- handle zero special case --
    emitter.label("__rt_itoa_positive");
    emitter.instruction("cbnz x0, __rt_itoa_loop");                             // if value != 0, start digit extraction loop
    emitter.instruction("mov w12, #48");                                        // ASCII '0'
    emitter.instruction("strb w12, [x9]");                                      // store '0' at current position
    emitter.instruction("sub x9, x9, #1");                                      // move write cursor left
    emitter.instruction("mov x10, #1");                                         // digit count = 1
    emitter.instruction("b __rt_itoa_done");                                    // skip to finalization

    // -- extract digits right-to-left via repeated division by 10 --
    emitter.label("__rt_itoa_loop");
    emitter.instruction("cbz x0, __rt_itoa_sign");                              // if quotient is 0, all digits extracted
    emitter.instruction("mov x12, #10");                                        // divisor = 10
    emitter.instruction("udiv x13, x0, x12");                                   // quotient = value / 10
    emitter.instruction("msub x14, x13, x12, x0");                              // remainder = value - (quotient * 10)
    emitter.instruction("add x14, x14, #48");                                   // convert remainder to ASCII digit
    emitter.instruction("strb w14, [x9]");                                      // store digit at current position
    emitter.instruction("sub x9, x9, #1");                                      // move write cursor left (right-to-left)
    emitter.instruction("add x10, x10, #1");                                    // increment digit count
    emitter.instruction("mov x0, x13");                                         // value = quotient for next iteration
    emitter.instruction("b __rt_itoa_loop");                                    // continue extracting digits

    // -- prepend minus sign if negative --
    emitter.label("__rt_itoa_sign");
    emitter.instruction("cbz x11, __rt_itoa_done");                             // skip if not negative
    emitter.instruction("mov w12, #45");                                        // ASCII '-'
    emitter.instruction("strb w12, [x9]");                                      // store minus sign
    emitter.instruction("sub x9, x9, #1");                                      // move cursor left past the sign
    emitter.instruction("add x10, x10, #1");                                    // count the sign in total length

    // -- finalize: update concat_buf offset and return ptr/len --
    emitter.label("__rt_itoa_done");
    emitter.instruction("add x8, x8, #21");                                     // advance concat_off by scratch area size
    emitter.instruction("str x8, [x6]");                                        // store updated offset back to _concat_off
    emitter.instruction("add x1, x9, #1");                                      // result ptr = one past last written position
    emitter.instruction("mov x2, x10");                                         // result length = digit count

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_itoa_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: itoa ---");
    emitter.label_global("__rt_itoa");

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // save the caller frame pointer before using rbp locally
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the routine

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat buffer offset
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("add r10, r9");                                         // compute the current concat buffer write position
    emitter.instruction("add r10, 20");                                         // advance to the end of the 21-byte scratch area for right-to-left digit writes

    // -- initialize counters --
    emitter.instruction("xor ecx, ecx");                                        // digit count = 0
    emitter.instruction("xor r11d, r11d");                                      // negative flag = 0

    // -- handle sign --
    emitter.instruction("test rax, rax");                                       // check whether the input integer is negative
    emitter.instruction("jns __rt_itoa_positive");                              // skip negation when the input is already non-negative
    emitter.instruction("mov r11d, 1");                                         // remember that we need to prepend a minus sign later
    emitter.instruction("neg rax");                                             // negate the value so the digit loop can use unsigned division

    // -- handle zero special case --
    emitter.label("__rt_itoa_positive");
    emitter.instruction("test rax, rax");                                       // check whether the absolute value is zero
    emitter.instruction("jne __rt_itoa_loop");                                  // start the digit extraction loop when the value is non-zero
    emitter.instruction("mov BYTE PTR [r10], 48");                              // store ASCII '0' into the scratch area
    emitter.instruction("dec r10");                                             // move the write cursor left after the single digit
    emitter.instruction("mov ecx, 1");                                          // digit count = 1 for the zero special case
    emitter.instruction("jmp __rt_itoa_done");                                  // skip the generic digit extraction loop

    // -- extract digits right-to-left via repeated division by 10 --
    emitter.label("__rt_itoa_loop");
    emitter.instruction("mov esi, 10");                                         // divisor = 10 for decimal digit extraction
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before unsigned division
    emitter.instruction("div rsi");                                             // quotient -> rax, remainder -> rdx
    emitter.instruction("add dl, 48");                                          // convert the decimal remainder to its ASCII digit
    emitter.instruction("mov BYTE PTR [r10], dl");                              // store the digit at the current scratch position
    emitter.instruction("dec r10");                                             // move the write cursor left for the next digit
    emitter.instruction("inc ecx");                                             // increment the output length after storing one digit
    emitter.instruction("test rax, rax");                                       // check whether more quotient digits remain
    emitter.instruction("jne __rt_itoa_loop");                                  // continue until the quotient reaches zero

    // -- prepend minus sign if negative --
    emitter.label("__rt_itoa_sign");
    emitter.instruction("test r11, r11");                                       // check whether the original value was negative
    emitter.instruction("jz __rt_itoa_done");                                   // skip sign emission for non-negative values
    emitter.instruction("mov BYTE PTR [r10], 45");                              // store ASCII '-' before the first digit
    emitter.instruction("dec r10");                                             // move the cursor left past the sign
    emitter.instruction("inc ecx");                                             // count the sign in the returned string length

    // -- finalize: update concat_buf offset and return ptr/len --
    emitter.label("__rt_itoa_done");
    emitter.instruction("add r9, 21");                                          // advance concat_off by the fixed scratch area size
    emitter.instruction("mov QWORD PTR [r8], r9");                              // store the updated concat buffer offset back to global storage
    emitter.instruction("lea rax, [r10 + 1]");                                  // return the string pointer as one byte past the last decremented position
    emitter.instruction("mov rdx, rcx");                                        // return the string length in the second string-result register

    // -- restore frame and return --
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return to the caller with rax=ptr and rdx=len
}
