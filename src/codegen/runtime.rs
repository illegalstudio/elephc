use super::emit::Emitter;

pub fn emit_runtime(emitter: &mut Emitter) {
    emit_itoa(emitter);
}

/// itoa: convert signed 64-bit integer to decimal string.
/// Input:  x0 = integer value
/// Output: x1 = pointer to string, x2 = length
/// Uses a 21-byte stack buffer (max digits for i64 + sign + null).
fn emit_itoa(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: itoa ---");
    emitter.comment("Input: x0 = integer value");
    emitter.comment("Output: x1 = pointer to string, x2 = length");
    emitter.label("__rt_itoa");
    // Prologue
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");

    // Buffer at sp+0..sp+20 (21 bytes)
    // x9 = pointer to end of buffer
    emitter.instruction("add x9, sp, #20");
    // x10 = digit count
    emitter.instruction("mov x10, #0");
    // x11 = is_negative flag
    emitter.instruction("mov x11, #0");

    // Handle negative
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.ge __rt_itoa_positive");
    emitter.instruction("mov x11, #1");
    emitter.instruction("neg x0, x0");

    emitter.label("__rt_itoa_positive");
    // Handle zero
    emitter.instruction("cbnz x0, __rt_itoa_loop");
    // x0 is 0: store '0'
    emitter.instruction("mov w12, #48");
    emitter.instruction("strb w12, [x9]");
    emitter.instruction("mov x10, #1");
    emitter.instruction("b __rt_itoa_done");

    // Digit extraction loop
    emitter.label("__rt_itoa_loop");
    emitter.instruction("cbz x0, __rt_itoa_sign");
    emitter.instruction("mov x12, #10");
    emitter.instruction("udiv x13, x0, x12");
    emitter.instruction("msub x14, x13, x12, x0"); // x14 = x0 % 10
    emitter.instruction("add x14, x14, #48"); // to ASCII
    emitter.instruction("strb w14, [x9]");
    emitter.instruction("sub x9, x9, #1");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("mov x0, x13");
    emitter.instruction("b __rt_itoa_loop");

    // Prepend '-' if negative
    emitter.label("__rt_itoa_sign");
    emitter.instruction("cbz x11, __rt_itoa_done");
    emitter.instruction("mov w12, #45"); // '-'
    emitter.instruction("strb w12, [x9]");
    emitter.instruction("sub x9, x9, #1");
    emitter.instruction("add x10, x10, #1");

    emitter.label("__rt_itoa_done");
    // x1 = start of string = x9 + 1 (we went one past)
    // But if we entered via zero path, x9 is still at sp+20
    // For zero: x1 = sp+20, x2 = 1
    // For loop: digits are stored right-to-left ending at x9+1
    emitter.instruction("add x1, x9, #1");
    emitter.instruction("mov x2, x10");

    // Epilogue
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
}
