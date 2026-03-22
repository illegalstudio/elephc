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

/// number_format: format a float with decimals and custom separators.
/// Input:  d0 = number, x1 = decimals, x2 = dec_point char, x3 = thousands_sep char (0=none)
/// Output: x1 = pointer to string, x2 = length
/// Uses snprintf for decimal formatting, then inserts thousands separators.
pub fn emit_number_format(emitter: &mut Emitter) {
    // Stack frame layout (128 bytes):
    //   [sp+0..47]  snprintf buffer (48 bytes)
    //   [sp+64..68] format string "%.Nf\0"
    //   [sp+72]     result start ptr
    //   [sp+80]     raw_len
    //   [sp+88]     number (d0)
    //   [sp+96]     decimals
    //   [sp+100]    dec_point char
    //   [sp+104]    thousands_sep char
    //   [sp+112]    saved x29, x30
    emitter.blank();
    emitter.comment("--- runtime: number_format ---");
    emitter.label("__rt_number_format");
    emitter.instruction("sub sp, sp, #128");
    emitter.instruction("stp x29, x30, [sp, #112]");
    emitter.instruction("add x29, sp, #112");

    // Save args
    emitter.instruction("str x1, [sp, #96]");   // decimals
    emitter.instruction("str d0, [sp, #88]");    // number
    emitter.instruction("str x2, [sp, #100]");   // dec_point char
    emitter.instruction("str x3, [sp, #104]");   // thousands_sep char

    // Build format string: "%.<decimals>f"
    emitter.instruction("mov w9, #37");  // '%'
    emitter.instruction("strb w9, [sp, #64]");
    emitter.instruction("mov w9, #46");  // '.'
    emitter.instruction("strb w9, [sp, #65]");
    emitter.instruction("ldr x9, [sp, #96]");
    emitter.instruction("add w9, w9, #48"); // '0' + decimals
    emitter.instruction("strb w9, [sp, #66]");
    emitter.instruction("mov w9, #102"); // 'f'
    emitter.instruction("strb w9, [sp, #67]");
    emitter.instruction("strb wzr, [sp, #68]");

    // snprintf(buf, 48, fmt, d0) — Apple ARM64 variadic ABI
    emitter.instruction("add x0, sp, #0");
    emitter.instruction("mov x1, #48");
    emitter.instruction("add x2, sp, #64");
    emitter.instruction("ldr d0, [sp, #88]");
    emitter.instruction("str d0, [sp, #-16]!");
    emitter.instruction("bl _snprintf");
    emitter.instruction("add sp, sp, #16");
    emitter.instruction("str x0, [sp, #80]"); // raw_len

    // Set up concat_buf destination
    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x10, x7, x8"); // dest ptr
    emitter.instruction("str x10, [sp, #72]"); // save result start

    // Scan raw string: find integer part length
    emitter.instruction("add x11, sp, #0"); // src ptr
    emitter.instruction("ldr x12, [sp, #80]"); // raw_len
    emitter.instruction("mov x13, #0"); // int_len

    // Handle negative sign
    emitter.instruction("ldrb w14, [x11]");
    emitter.instruction("cmp w14, #45"); // '-'
    emitter.instruction("b.ne __rt_nf_count");
    emitter.instruction("strb w14, [x10], #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("sub x12, x12, #1");

    emitter.label("__rt_nf_count");
    emitter.instruction("mov x15, x11"); // start of integer digits
    emitter.instruction("mov x13, #0");
    emitter.label("__rt_nf_count_loop");
    emitter.instruction("cbz x12, __rt_nf_count_done");
    emitter.instruction("ldrb w14, [x11, x13]");
    emitter.instruction("cmp w14, #46"); // '.' (snprintf always uses '.')
    emitter.instruction("b.eq __rt_nf_count_done");
    emitter.instruction("add x13, x13, #1");
    emitter.instruction("sub x12, x12, #1");
    emitter.instruction("b __rt_nf_count_loop");

    emitter.label("__rt_nf_count_done");
    // x13=int digit count, x15=start of digits, x12=remaining (decimal part)
    // Copy integer digits with thousands separator
    emitter.instruction("mov x16, #0"); // src index
    emitter.instruction("mov x17, #3");
    emitter.instruction("udiv x18, x13, x17");
    emitter.instruction("msub x14, x18, x17, x13"); // first group size = int_len % 3
    emitter.instruction("cbnz x14, __rt_nf_copy_int");
    emitter.instruction("mov x14, #3");

    emitter.label("__rt_nf_copy_int");
    emitter.instruction("cmp x16, x13");
    emitter.instruction("b.ge __rt_nf_decimal");
    // Insert thousands separator between groups
    emitter.instruction("cbz x16, __rt_nf_no_sep");
    emitter.instruction("cmp x14, #0");
    emitter.instruction("b.ne __rt_nf_no_sep");
    // Check if thousands_sep is set (non-zero)
    emitter.instruction("ldr x9, [sp, #104]");
    emitter.instruction("cbz x9, __rt_nf_no_sep_reset"); // skip if no separator
    emitter.instruction("strb w9, [x10], #1");
    emitter.label("__rt_nf_no_sep_reset");
    emitter.instruction("mov x14, #3");

    emitter.label("__rt_nf_no_sep");
    emitter.instruction("ldrb w9, [x15, x16]");
    emitter.instruction("strb w9, [x10], #1");
    emitter.instruction("add x16, x16, #1");
    emitter.instruction("sub x14, x14, #1");
    emitter.instruction("b __rt_nf_copy_int");

    emitter.label("__rt_nf_decimal");
    // Copy decimal part, replacing '.' with custom dec_point
    emitter.instruction("add x15, x15, x13");
    emitter.label("__rt_nf_copy_dec");
    emitter.instruction("cbz x12, __rt_nf_done");
    emitter.instruction("ldrb w9, [x15], #1");
    // Replace '.' with custom decimal point
    emitter.instruction("cmp w9, #46"); // '.'
    emitter.instruction("b.ne __rt_nf_dec_store");
    emitter.instruction("ldr x9, [sp, #100]"); // dec_point char
    emitter.label("__rt_nf_dec_store");
    emitter.instruction("strb w9, [x10], #1");
    emitter.instruction("sub x12, x12, #1");
    emitter.instruction("b __rt_nf_copy_dec");

    emitter.label("__rt_nf_done");
    emitter.instruction("ldr x1, [sp, #72]");
    emitter.instruction("sub x2, x10, x1");
    // Update concat_off
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("add x8, x8, x2");
    emitter.instruction("str x8, [x6]");

    emitter.instruction("ldp x29, x30, [sp, #112]");
    emitter.instruction("add sp, sp, #128");
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
