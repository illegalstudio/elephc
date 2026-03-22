use crate::codegen::emit::Emitter;

/// atoi: parse a string to a signed 64-bit integer.
/// Input:  x1 = string pointer, x2 = string length
/// Output: x0 = integer value
pub fn emit_atoi(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: atoi ---");
    emitter.label("__rt_atoi");

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
