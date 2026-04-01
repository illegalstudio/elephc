use crate::codegen::emit::Emitter;

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
    emitter.label_global("__rt_number_format");

    // -- set up stack frame (128 bytes) --
    emitter.instruction("sub sp, sp, #128");                                    // allocate 128 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish new frame pointer

    // -- save input arguments --
    emitter.instruction("str x1, [sp, #96]");                                   // save decimals count
    emitter.instruction("str d0, [sp, #88]");                                   // save the floating-point number
    emitter.instruction("str x2, [sp, #100]");                                  // save decimal point character
    emitter.instruction("str x3, [sp, #104]");                                  // save thousands separator character

    // -- build format string "%.Nf" at [sp+64] --
    emitter.instruction("mov w9, #37");                                         // ASCII '%'
    emitter.instruction("strb w9, [sp, #64]");                                  // write '%' to format string
    emitter.instruction("mov w9, #46");                                         // ASCII '.'
    emitter.instruction("strb w9, [sp, #65]");                                  // write '.' to format string
    emitter.instruction("ldr x9, [sp, #96]");                                   // load decimals count
    emitter.instruction("add w9, w9, #48");                                     // convert to ASCII digit ('0' + N)
    emitter.instruction("strb w9, [sp, #66]");                                  // write decimal count digit
    emitter.instruction("mov w9, #102");                                        // ASCII 'f'
    emitter.instruction("strb w9, [sp, #67]");                                  // write 'f' format specifier
    emitter.instruction("strb wzr, [sp, #68]");                                 // null-terminate the format string

    // -- call snprintf(buf, 48, fmt, d0) --
    emitter.instruction("add x0, sp, #0");                                      // x0 = output buffer at start of stack frame
    emitter.instruction("mov x1, #48");                                         // buffer size = 48 bytes
    emitter.instruction("add x2, sp, #64");                                     // x2 = format string pointer
    emitter.instruction("ldr d0, [sp, #88]");                                   // reload the float value
    emitter.instruction("str d0, [sp, #-16]!");                                 // push double for variadic ABI, adjust sp
    emitter.instruction("bl _snprintf");                                        // call snprintf; returns char count in x0
    emitter.instruction("add sp, sp, #16");                                     // pop the variadic argument from stack
    emitter.instruction("str x0, [sp, #80]");                                   // save raw string length

    // -- set up concat_buf destination --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve exact address
    emitter.instruction("ldr x8, [x6]");                                        // load current concat_buf write offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve exact buffer base
    emitter.instruction("add x10, x7, x8");                                     // compute destination pointer
    emitter.instruction("str x10, [sp, #72]");                                  // save result start pointer

    // -- scan raw string to find integer part length --
    emitter.instruction("add x11, sp, #0");                                     // x11 = source ptr (snprintf output)
    emitter.instruction("ldr x12, [sp, #80]");                                  // x12 = raw string length
    emitter.instruction("mov x13, #0");                                         // x13 = integer part digit count

    // -- handle leading minus sign --
    emitter.instruction("ldrb w14, [x11]");                                     // load first character
    emitter.instruction("cmp w14, #45");                                        // check if it's '-' (minus sign)
    emitter.instruction("b.ne __rt_nf_count");                                  // skip if not negative
    emitter.instruction("strb w14, [x10], #1");                                 // copy '-' to output, advance dest
    emitter.instruction("add x11, x11, #1");                                    // advance source past '-'
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining length

    // -- count integer digits (before decimal point) --
    emitter.label("__rt_nf_count");
    emitter.instruction("mov x15, x11");                                        // save start of integer digits
    emitter.instruction("mov x13, #0");                                         // reset digit counter
    emitter.label("__rt_nf_count_loop");
    emitter.instruction("cbz x12, __rt_nf_count_done");                         // if no chars remain, done counting
    emitter.instruction("ldrb w14, [x11, x13]");                                // load char at current offset
    emitter.instruction("cmp w14, #46");                                        // check if it's '.' (decimal point)
    emitter.instruction("b.eq __rt_nf_count_done");                             // stop counting at decimal point
    emitter.instruction("add x13, x13, #1");                                    // increment integer digit count
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining chars
    emitter.instruction("b __rt_nf_count_loop");                                // continue scanning

    emitter.label("__rt_nf_count_done");
    // x13=int digit count, x15=start of digits, x12=remaining (decimal part)

    // -- copy integer digits with thousands separator --
    emitter.instruction("mov x16, #0");                                         // source index into integer digits
    emitter.instruction("mov x17, #3");                                         // group size for thousands
    emitter.instruction("udiv x18, x13, x17");                                  // number of complete 3-digit groups
    emitter.instruction("msub x14, x18, x17, x13");                             // first group size = digit_count % 3
    emitter.instruction("cbnz x14, __rt_nf_copy_int");                          // if first group non-empty, start copying
    emitter.instruction("mov x14, #3");                                         // first group is full 3 digits

    emitter.label("__rt_nf_copy_int");
    emitter.instruction("cmp x16, x13");                                        // check if all integer digits copied
    emitter.instruction("b.ge __rt_nf_decimal");                                // if done, move to decimal part

    // -- insert thousands separator between groups --
    emitter.instruction("cbz x16, __rt_nf_no_sep");                             // skip separator before first digit
    emitter.instruction("cmp x14, #0");                                         // check if current group is exhausted
    emitter.instruction("b.ne __rt_nf_no_sep");                                 // group not done, no separator yet
    emitter.instruction("ldr x9, [sp, #104]");                                  // load thousands separator char
    emitter.instruction("cbz x9, __rt_nf_no_sep_reset");                        // skip if separator is 0 (none)
    emitter.instruction("strb w9, [x10], #1");                                  // write separator to output, advance dest
    emitter.label("__rt_nf_no_sep_reset");
    emitter.instruction("mov x14, #3");                                         // reset group counter for next 3 digits

    emitter.label("__rt_nf_no_sep");
    emitter.instruction("ldrb w9, [x15, x16]");                                 // load next integer digit from source
    emitter.instruction("strb w9, [x10], #1");                                  // write digit to output, advance dest
    emitter.instruction("add x16, x16, #1");                                    // advance source index
    emitter.instruction("sub x14, x14, #1");                                    // decrement group counter
    emitter.instruction("b __rt_nf_copy_int");                                  // continue copying integer digits

    // -- copy decimal part, replacing '.' with custom separator --
    emitter.label("__rt_nf_decimal");
    emitter.instruction("add x15, x15, x13");                                   // advance source past integer digits
    emitter.label("__rt_nf_copy_dec");
    emitter.instruction("cbz x12, __rt_nf_done");                               // if no decimal chars remain, done
    emitter.instruction("ldrb w9, [x15], #1");                                  // load next decimal char, advance source
    emitter.instruction("cmp w9, #46");                                         // check if it's '.' (snprintf decimal point)
    emitter.instruction("b.ne __rt_nf_dec_store");                              // if not '.', store as-is
    emitter.instruction("ldr x9, [sp, #100]");                                  // replace with custom decimal point char
    emitter.label("__rt_nf_dec_store");
    emitter.instruction("strb w9, [x10], #1");                                  // write char to output, advance dest
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining chars
    emitter.instruction("b __rt_nf_copy_dec");                                  // continue copying decimal part

    // -- finalize: compute length and update concat_off --
    emitter.label("__rt_nf_done");
    emitter.instruction("ldr x1, [sp, #72]");                                   // load result start pointer
    emitter.instruction("sub x2, x10, x1");                                     // result length = dest_end - dest_start
    emitter.instruction("ldr x8, [x6]");                                        // load current concat_off
    emitter.instruction("add x8, x8, x2");                                      // advance offset by result length
    emitter.instruction("str x8, [x6]");                                        // store updated concat_off

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
