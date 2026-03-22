use crate::codegen::emit::Emitter;

/// itoa: convert signed 64-bit integer to decimal string.
/// Input:  x0 = integer value
/// Output: x1 = pointer to string, x2 = length
pub fn emit_itoa(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: itoa ---");
    emitter.label("__rt_itoa");
    emitter.instruction("sub sp, sp, #16");
    emitter.instruction("stp x29, x30, [sp]");
    emitter.instruction("mov x29, sp");

    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x9, x7, x8");
    emitter.instruction("add x9, x9, #20");

    emitter.instruction("mov x10, #0");
    emitter.instruction("mov x11, #0");

    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.ge __rt_itoa_positive");
    emitter.instruction("mov x11, #1");
    emitter.instruction("neg x0, x0");

    emitter.label("__rt_itoa_positive");
    emitter.instruction("cbnz x0, __rt_itoa_loop");
    emitter.instruction("mov w12, #48");
    emitter.instruction("strb w12, [x9]");
    emitter.instruction("sub x9, x9, #1");
    emitter.instruction("mov x10, #1");
    emitter.instruction("b __rt_itoa_done");

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

    emitter.label("__rt_itoa_sign");
    emitter.instruction("cbz x11, __rt_itoa_done");
    emitter.instruction("mov w12, #45");
    emitter.instruction("strb w12, [x9]");
    emitter.instruction("sub x9, x9, #1");
    emitter.instruction("add x10, x10, #1");

    emitter.label("__rt_itoa_done");
    emitter.instruction("add x8, x8, #21");
    emitter.instruction("str x8, [x6]");
    emitter.instruction("add x1, x9, #1");
    emitter.instruction("mov x2, x10");
    emitter.instruction("ldp x29, x30, [sp]");
    emitter.instruction("add sp, sp, #16");
    emitter.instruction("ret");
}

/// concat: concatenate two strings.
/// Input:  x1=left_ptr, x2=left_len, x3=right_ptr, x4=right_len
/// Output: x1=result_ptr, x2=result_len
pub fn emit_concat(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat ---");
    emitter.label("__rt_concat");
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #48]");
    emitter.instruction("add x29, sp, #48");

    emitter.instruction("stp x1, x2, [sp, #0]");
    emitter.instruction("stp x3, x4, [sp, #16]");
    emitter.instruction("add x5, x2, x4");
    emitter.instruction("str x5, [sp, #32]");

    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x9, x7, x8");
    emitter.instruction("str x9, [sp, #40]");

    emitter.instruction("ldp x1, x2, [sp, #0]");
    emitter.instruction("mov x10, x9");
    emitter.label("__rt_concat_cl");
    emitter.instruction("cbz x2, __rt_concat_cr_setup");
    emitter.instruction("ldrb w11, [x1], #1");
    emitter.instruction("strb w11, [x10], #1");
    emitter.instruction("sub x2, x2, #1");
    emitter.instruction("b __rt_concat_cl");

    emitter.label("__rt_concat_cr_setup");
    emitter.instruction("ldp x3, x4, [sp, #16]");
    emitter.label("__rt_concat_cr");
    emitter.instruction("cbz x4, __rt_concat_done");
    emitter.instruction("ldrb w11, [x3], #1");
    emitter.instruction("strb w11, [x10], #1");
    emitter.instruction("sub x4, x4, #1");
    emitter.instruction("b __rt_concat_cr");

    emitter.label("__rt_concat_done");
    emitter.instruction("ldr x5, [sp, #32]");
    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("add x8, x8, x5");
    emitter.instruction("str x8, [x6]");

    emitter.instruction("ldr x1, [sp, #40]");
    emitter.instruction("ldr x2, [sp, #32]");
    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}

/// ftoa: convert double-precision float to string.
/// Input:  d0 = float value
/// Output: x1 = pointer to string, x2 = length
/// Uses _snprintf with "%.14G" format.
/// On Apple ARM64 variadic ABI, the double goes on the stack.
pub fn emit_ftoa(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ftoa ---");
    emitter.label("__rt_ftoa");
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #48]");
    emitter.instruction("add x29, sp, #48");

    // Get current concat_buf position
    emitter.instruction("adrp x9, _concat_off@PAGE");
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("str x10, [sp, #32]");  // save original offset
    emitter.instruction("str x9, [sp, #40]");   // save offset ptr

    emitter.instruction("adrp x11, _concat_buf@PAGE");
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");
    emitter.instruction("add x0, x11, x10"); // buf ptr = concat_buf + offset
    emitter.instruction("str x0, [sp, #24]"); // save buf start

    // Call snprintf(buf, 32, "%.14G", double)
    // On Apple ARM64 variadic ABI, the double must go on the stack
    emitter.instruction("mov x1, #32");         // buffer size
    emitter.instruction("adrp x2, _fmt_g@PAGE");
    emitter.instruction("add x2, x2, _fmt_g@PAGEOFF");
    // Store the double on the stack for variadic call
    emitter.instruction("str d0, [sp]");
    emitter.instruction("bl _snprintf");

    // x0 = number of chars written
    emitter.instruction("mov x2, x0"); // length

    // Update concat_off
    emitter.instruction("ldr x9, [sp, #40]");   // offset ptr
    emitter.instruction("ldr x10, [sp, #32]");   // original offset
    emitter.instruction("add x10, x10, x2");
    emitter.instruction("str x10, [x9]");

    // x1 = buf start
    emitter.instruction("ldr x1, [sp, #24]");

    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}

/// str_eq: compare two strings for equality.
/// Input:  x1=ptr_a, x2=len_a, x3=ptr_b, x4=len_b
/// Output: x0 = 1 if equal, 0 if not
pub fn emit_str_eq(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_eq ---");
    emitter.label("__rt_str_eq");

    // Compare lengths first
    emitter.instruction("cmp x2, x4");
    emitter.instruction("b.ne __rt_str_eq_false");

    // Same length — compare bytes
    emitter.instruction("cbz x2, __rt_str_eq_true"); // both empty → equal
    emitter.label("__rt_str_eq_loop");
    emitter.instruction("ldrb w5, [x1], #1");
    emitter.instruction("ldrb w6, [x3], #1");
    emitter.instruction("cmp w5, w6");
    emitter.instruction("b.ne __rt_str_eq_false");
    emitter.instruction("sub x2, x2, #1");
    emitter.instruction("cbnz x2, __rt_str_eq_loop");

    emitter.label("__rt_str_eq_true");
    emitter.instruction("mov x0, #1");
    emitter.instruction("ret");

    emitter.label("__rt_str_eq_false");
    emitter.instruction("mov x0, #0");
    emitter.instruction("ret");
}

/// atoi: parse a string to a signed 64-bit integer.
/// Input:  x1 = string pointer, x2 = string length
/// Output: x0 = integer value
pub fn emit_atoi(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: atoi ---");
    emitter.label("__rt_atoi");
    emitter.instruction("mov x0, #0");
    emitter.instruction("mov x3, #0");
    emitter.instruction("cbz x2, __rt_atoi_done");

    emitter.instruction("ldrb w4, [x1]");
    emitter.instruction("cmp w4, #45");
    emitter.instruction("b.ne __rt_atoi_loop");
    emitter.instruction("mov x3, #1");
    emitter.instruction("add x1, x1, #1");
    emitter.instruction("sub x2, x2, #1");

    emitter.label("__rt_atoi_loop");
    emitter.instruction("cbz x2, __rt_atoi_sign");
    emitter.instruction("ldrb w4, [x1], #1");
    emitter.instruction("sub w4, w4, #48");
    emitter.instruction("cmp w4, #9");
    emitter.instruction("b.hi __rt_atoi_sign");
    emitter.instruction("mov x5, #10");
    emitter.instruction("mul x0, x0, x5");
    emitter.instruction("add x0, x0, x4");
    emitter.instruction("sub x2, x2, #1");
    emitter.instruction("b __rt_atoi_loop");

    emitter.label("__rt_atoi_sign");
    emitter.instruction("cbz x3, __rt_atoi_done");
    emitter.instruction("neg x0, x0");

    emitter.label("__rt_atoi_done");
    emitter.instruction("ret");
}
