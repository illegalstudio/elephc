use crate::codegen::{emit::Emitter, platform::Arch};

/// atoi: parse a string to a signed 64-bit integer.
/// Input:  AArch64 uses x1/x2, x86_64 uses rax/rdx for ptr/len
/// Output: AArch64 returns x0, x86_64 returns rax
pub fn emit_atoi(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_atoi_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: atoi ---");
    emitter.label_global("__rt_atoi");

    // -- initialize result and sign flag --
    emitter.instruction("mov x0, #0");                                          // initialize result accumulator to zero
    emitter.instruction("mov x3, #0");                                          // negative flag = 0 (positive)
    emitter.instruction("cbz x2, __rt_atoi_done");                              // if string is empty, return 0

    // -- check for leading minus sign --
    emitter.instruction("ldrb w4, [x1]");                                       // load first character
    emitter.instruction("cmp w4, #45");                                         // check if it's '-' (minus sign)
    emitter.instruction("b.ne __rt_atoi_loop");                                 // not negative, start parsing digits
    emitter.instruction("mov x3, #1");                                          // set negative flag
    emitter.instruction("add x1, x1, #1");                                      // advance past the minus sign
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining length

    // -- parse digits: result = result * 10 + digit --
    emitter.label("__rt_atoi_loop");
    emitter.instruction("cbz x2, __rt_atoi_sign");                              // if no chars left, apply sign
    emitter.instruction("ldrb w4, [x1], #1");                                   // load next byte and advance pointer
    emitter.instruction("sub w4, w4, #48");                                     // convert ASCII to digit (subtract '0')
    emitter.instruction("cmp w4, #9");                                          // check if it's a valid digit (0-9)
    emitter.instruction("b.hi __rt_atoi_sign");                                 // if > 9 (non-digit), stop parsing
    emitter.instruction("mov x5, #10");                                         // multiplier = 10
    emitter.instruction("mul x0, x0, x5");                                      // shift accumulator left by one decimal place
    emitter.instruction("add x0, x0, x4");                                      // add current digit to accumulator
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining length
    emitter.instruction("b __rt_atoi_loop");                                    // continue parsing next character

    // -- apply sign if negative --
    emitter.label("__rt_atoi_sign");
    emitter.instruction("cbz x3, __rt_atoi_done");                              // if not negative, skip negation
    emitter.instruction("neg x0, x0");                                          // negate the result

    emitter.label("__rt_atoi_done");
    emitter.instruction("ret");                                                 // return to caller with result in x0
}

fn emit_atoi_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: atoi ---");
    emitter.label_global("__rt_atoi");

    // -- initialize result and parsing cursors --
    emitter.instruction("mov r8, rax");                                         // copy the incoming string pointer into a scratch cursor register
    emitter.instruction("mov rcx, rdx");                                        // copy the incoming string length into a decrementing loop counter
    emitter.instruction("mov rax, 0");                                          // initialize the integer accumulator to zero
    emitter.instruction("mov r9, 0");                                           // negative flag = 0 (positive)
    emitter.instruction("test rcx, rcx");                                       // check whether the input string is empty
    emitter.instruction("je __rt_atoi_done_linux_x86_64");                      // if the string is empty, return 0 immediately

    // -- check for leading minus sign --
    emitter.instruction("movzx r10, BYTE PTR [r8]");                            // load the first input byte without advancing the parsing cursor yet
    emitter.instruction("cmp r10, 45");                                         // check whether the first input byte is '-' (minus sign)
    emitter.instruction("jne __rt_atoi_loop_linux_x86_64");                     // if the string is not negative, jump directly into the digit loop
    emitter.instruction("mov r9, 1");                                           // remember that the parsed integer must be negated before returning
    emitter.instruction("add r8, 1");                                           // advance the parsing cursor past the leading minus sign
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining byte count after consuming the minus sign

    // -- parse digits: result = result * 10 + digit --
    emitter.label("__rt_atoi_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop parsing once the remaining byte count reaches zero
    emitter.instruction("je __rt_atoi_sign_linux_x86_64");                      // if no bytes remain, apply the sign and return
    emitter.instruction("movzx r10, BYTE PTR [r8]");                            // load the next input byte from the current parsing cursor
    emitter.instruction("add r8, 1");                                           // advance the parsing cursor to the following byte for the next iteration
    emitter.instruction("sub r10, 48");                                         // convert the ASCII digit into its integer value by subtracting '0'
    emitter.instruction("cmp r10, 9");                                          // check whether the converted byte still lies in the 0..9 digit range
    emitter.instruction("ja __rt_atoi_sign_linux_x86_64");                      // stop parsing as soon as a non-digit byte is encountered
    emitter.instruction("imul rax, rax, 10");                                   // shift the accumulator left by one decimal digit before adding the new digit
    emitter.instruction("add rax, r10");                                        // append the parsed digit to the accumulator
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining byte count after consuming this digit
    emitter.instruction("jmp __rt_atoi_loop_linux_x86_64");                     // continue parsing subsequent digits until the loop terminates

    // -- apply sign if negative --
    emitter.label("__rt_atoi_sign_linux_x86_64");
    emitter.instruction("test r9, r9");                                         // check whether the parsed number carried a leading minus sign
    emitter.instruction("je __rt_atoi_done_linux_x86_64");                      // skip negation when the parsed number is already positive
    emitter.instruction("neg rax");                                             // negate the accumulated value for negative decimal strings

    emitter.label("__rt_atoi_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return to caller with the parsed integer in rax
}
