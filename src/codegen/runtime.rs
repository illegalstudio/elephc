use super::emit::Emitter;

pub fn emit_runtime(emitter: &mut Emitter) {
    emit_itoa(emitter);
    emit_concat(emitter);
}

/// Returns BSS/data directives needed by runtime routines.
pub fn emit_runtime_data() -> String {
    let mut out = String::new();
    out.push_str(".comm _concat_buf, 4096, 3\n");
    out.push_str(".comm _concat_off, 8, 3\n");
    out
}

/// itoa: convert signed 64-bit integer to decimal string.
/// Input:  x0 = integer value
/// Output: x1 = pointer to string, x2 = length
fn emit_itoa(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: itoa ---");
    emitter.comment("Input: x0 = integer value");
    emitter.comment("Output: x1 = pointer to string, x2 = length");
    emitter.label("__rt_itoa");
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");

    // x9 = pointer to end of 21-byte stack buffer
    emitter.instruction("add x9, sp, #20");
    emitter.instruction("mov x10, #0"); // digit count
    emitter.instruction("mov x11, #0"); // is_negative

    // Handle negative
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.ge __rt_itoa_positive");
    emitter.instruction("mov x11, #1");
    emitter.instruction("neg x0, x0");

    emitter.label("__rt_itoa_positive");
    emitter.instruction("cbnz x0, __rt_itoa_loop");
    // Zero case
    emitter.instruction("mov w12, #48");
    emitter.instruction("strb w12, [x9]");
    emitter.instruction("sub x9, x9, #1");
    emitter.instruction("mov x10, #1");
    emitter.instruction("b __rt_itoa_done");

    // Digit extraction loop
    emitter.label("__rt_itoa_loop");
    emitter.instruction("cbz x0, __rt_itoa_sign");
    emitter.instruction("mov x12, #10");
    emitter.instruction("udiv x13, x0, x12");
    emitter.instruction("msub x14, x13, x12, x0");
    emitter.instruction("add x14, x14, #48");
    emitter.instruction("strb w14, [x9]");
    emitter.instruction("sub x9, x9, #1");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("mov x0, x13");
    emitter.instruction("b __rt_itoa_loop");

    // Sign
    emitter.label("__rt_itoa_sign");
    emitter.instruction("cbz x11, __rt_itoa_done");
    emitter.instruction("mov w12, #45");
    emitter.instruction("strb w12, [x9]");
    emitter.instruction("sub x9, x9, #1");
    emitter.instruction("add x10, x10, #1");

    emitter.label("__rt_itoa_done");
    emitter.instruction("add x1, x9, #1");
    emitter.instruction("mov x2, x10");
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
}

/// concat: concatenate two strings into a static buffer.
/// Input:  x1=left_ptr, x2=left_len, x3=right_ptr, x4=right_len
/// Output: x1=result_ptr, x2=result_len
/// Uses _concat_buf (4096 bytes) with bump offset _concat_off.
fn emit_concat(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat ---");
    emitter.comment("Input: x1=left_ptr, x2=left_len, x3=right_ptr, x4=right_len");
    emitter.comment("Output: x1=result_ptr, x2=result_len");
    emitter.label("__rt_concat");
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #48]");
    emitter.instruction("add x29, sp, #48");

    // Save inputs on stack
    emitter.instruction("stp x1, x2, [sp, #0]");  // left_ptr, left_len
    emitter.instruction("stp x3, x4, [sp, #16]"); // right_ptr, right_len

    // Total length
    emitter.instruction("add x5, x2, x4");
    emitter.instruction("str x5, [sp, #32]"); // total_len

    // Get buffer destination = _concat_buf + _concat_off
    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x9, x7, x8"); // dest
    emitter.instruction("str x9, [sp, #40]"); // save dest

    // Copy left string
    emitter.instruction("ldp x1, x2, [sp, #0]");
    emitter.instruction("mov x10, x9");
    emitter.label("__rt_concat_cl");
    emitter.instruction("cbz x2, __rt_concat_cr_setup");
    emitter.instruction("ldrb w11, [x1], #1");
    emitter.instruction("strb w11, [x10], #1");
    emitter.instruction("sub x2, x2, #1");
    emitter.instruction("b __rt_concat_cl");

    // Copy right string
    emitter.label("__rt_concat_cr_setup");
    emitter.instruction("ldp x3, x4, [sp, #16]");
    emitter.label("__rt_concat_cr");
    emitter.instruction("cbz x4, __rt_concat_done");
    emitter.instruction("ldrb w11, [x3], #1");
    emitter.instruction("strb w11, [x10], #1");
    emitter.instruction("sub x4, x4, #1");
    emitter.instruction("b __rt_concat_cr");

    // Update bump offset
    emitter.label("__rt_concat_done");
    emitter.instruction("ldr x5, [sp, #32]");
    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("add x8, x8, x5");
    emitter.instruction("str x8, [x6]");

    // Return result
    emitter.instruction("ldr x1, [sp, #40]");
    emitter.instruction("ldr x2, [sp, #32]");
    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}
