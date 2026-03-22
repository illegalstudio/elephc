use crate::codegen::emit::Emitter;

/// itoa: convert signed 64-bit integer to decimal string.
/// Input:  x0 = integer value
/// Output: x1 = pointer to string, x2 = length
pub fn emit_itoa(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: itoa ---");
    emitter.label("__rt_itoa");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- get concat_buf write position --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // add page offset to get exact address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset into concat_buf
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // add page offset to get buffer base
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

/// concat: concatenate two strings.
/// Input:  x1=left_ptr, x2=left_len, x3=right_ptr, x4=right_len
/// Output: x1=result_ptr, x2=result_len
pub fn emit_concat(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat ---");
    emitter.label("__rt_concat");

    // -- set up stack frame (64 bytes) --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save input arguments to stack --
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save left string ptr and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save right string ptr and length
    emitter.instruction("add x5, x2, x4");                                      // compute total result length
    emitter.instruction("str x5, [sp, #32]");                                   // save total length on stack

    // -- get concat_buf write position --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve exact buffer base address
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer: buf + offset
    emitter.instruction("str x9, [sp, #40]");                                   // save result start pointer on stack

    // -- copy left string bytes --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload left ptr and length
    emitter.instruction("mov x10, x9");                                         // set dest cursor to start of output
    emitter.label("__rt_concat_cl");
    emitter.instruction("cbz x2, __rt_concat_cr_setup");                        // if no bytes left, move to right string
    emitter.instruction("ldrb w11, [x1], #1");                                  // load byte from left string, advance src
    emitter.instruction("strb w11, [x10], #1");                                 // store byte to dest, advance dest
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining left bytes
    emitter.instruction("b __rt_concat_cl");                                    // continue copying left string

    // -- copy right string bytes --
    emitter.label("__rt_concat_cr_setup");
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload right ptr and length
    emitter.label("__rt_concat_cr");
    emitter.instruction("cbz x4, __rt_concat_done");                            // if no bytes left, concatenation complete
    emitter.instruction("ldrb w11, [x3], #1");                                  // load byte from right string, advance src
    emitter.instruction("strb w11, [x10], #1");                                 // store byte to dest, advance dest
    emitter.instruction("sub x4, x4, #1");                                      // decrement remaining right bytes
    emitter.instruction("b __rt_concat_cr");                                    // continue copying right string

    // -- update concat_buf offset and return result --
    emitter.label("__rt_concat_done");
    emitter.instruction("ldr x5, [sp, #32]");                                   // reload total result length
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve exact address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("add x8, x8, x5");                                      // advance offset by total length written
    emitter.instruction("str x8, [x6]");                                        // store updated offset

    // -- set return values and restore frame --
    emitter.instruction("ldr x1, [sp, #40]");                                   // return result pointer (start of output)
    emitter.instruction("ldr x2, [sp, #32]");                                   // return result length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
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

    // -- set up stack frame (64 bytes) --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- get current concat_buf position --
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    emitter.instruction("str x10, [sp, #32]");                                  // save original offset on stack
    emitter.instruction("str x9, [sp, #40]");                                   // save offset variable address on stack

    emitter.instruction("adrp x11, _concat_buf@PAGE");                          // load page address of concat buffer
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                   // resolve exact buffer base address
    emitter.instruction("add x0, x11, x10");                                    // compute output buffer: concat_buf + offset
    emitter.instruction("str x0, [sp, #24]");                                   // save output buffer start on stack

    // -- call snprintf(buf, 32, "%.14G", double) --
    emitter.instruction("mov x1, #32");                                         // buffer size limit = 32 bytes
    emitter.instruction("adrp x2, _fmt_g@PAGE");                                // load page address of format string "%.14G"
    emitter.instruction("add x2, x2, _fmt_g@PAGEOFF");                          // resolve exact address of format string
    // -- Apple ARM64 variadic ABI: float arg goes on stack, not in SIMD reg --
    emitter.instruction("str d0, [sp]");                                        // push double onto stack for variadic call
    emitter.instruction("bl _snprintf");                                        // call snprintf; returns char count in x0

    // -- x0 = number of chars written --
    emitter.instruction("mov x2, x0");                                          // save string length as return value

    // -- update concat_off by chars written --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload offset variable address
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload original offset
    emitter.instruction("add x10, x10, x2");                                    // new offset = original + chars written
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- set return pointer --
    emitter.instruction("ldr x1, [sp, #24]");                                   // return pointer to start of formatted string

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// str_eq: compare two strings for equality.
/// Input:  x1=ptr_a, x2=len_a, x3=ptr_b, x4=len_b
/// Output: x0 = 1 if equal, 0 if not
pub fn emit_str_eq(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_eq ---");
    emitter.label("__rt_str_eq");

    // -- quick length check: different lengths means not equal --
    emitter.instruction("cmp x2, x4");                                          // compare lengths of both strings
    emitter.instruction("b.ne __rt_str_eq_false");                              // if lengths differ, strings can't be equal

    // -- byte-by-byte comparison --
    emitter.instruction("cbz x2, __rt_str_eq_true");                            // if both empty (len=0), they're equal
    emitter.label("__rt_str_eq_loop");
    emitter.instruction("ldrb w5, [x1], #1");                                   // load byte from string A, advance pointer
    emitter.instruction("ldrb w6, [x3], #1");                                   // load byte from string B, advance pointer
    emitter.instruction("cmp w5, w6");                                          // compare the two bytes
    emitter.instruction("b.ne __rt_str_eq_false");                              // mismatch found, strings not equal
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining byte count
    emitter.instruction("cbnz x2, __rt_str_eq_loop");                           // if bytes remain, continue comparing

    // -- strings are equal --
    emitter.label("__rt_str_eq_true");
    emitter.instruction("mov x0, #1");                                          // return 1 (true: strings are equal)
    emitter.instruction("ret");                                                 // return to caller

    // -- strings are not equal --
    emitter.label("__rt_str_eq_false");
    emitter.instruction("mov x0, #0");                                          // return 0 (false: strings differ)
    emitter.instruction("ret");                                                 // return to caller
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
