use crate::codegen::emit::Emitter;

/// sprintf: format a string with typed arguments from the stack.
/// Input: x0=arg_count, x1=fmt_ptr, x2=fmt_len, args on stack (16 bytes each)
/// Output: x1=result_ptr, x2=result_len
/// Each stack arg is: [value, type_tag] where type_tag: 0=int, 1=str(len<<8), 2=float, 3=bool
/// The runtime pops arg_count*16 bytes from the caller's stack after processing.
///
/// This implementation delegates each format specifier to libc's snprintf for
/// correct handling of width, precision, padding, and alignment modifiers.
/// On Apple ARM64, snprintf variadic arguments are passed on the stack at [sp].
pub fn emit_sprintf(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf ---");
    emitter.label_global("__rt_sprintf");

    // Frame layout (288 bytes):
    //   sp+0..7     = variadic arg slot for snprintf (must be at sp)
    //   sp+8..15    = (padding for 16-byte alignment of variadic)
    //   sp+16..23   = saved x19
    //   sp+24..31   = saved x20
    //   sp+32..39   = saved x21
    //   sp+40..47   = saved x22
    //   sp+48..55   = saved x23
    //   sp+56..63   = saved x24
    //   sp+64..71   = saved x25
    //   sp+72..79   = saved x26
    //   sp+80..111  = mini format string buffer (32 bytes)
    //   sp+112..239 = snprintf output buffer (128 bytes)
    //   sp+240..367 = string null-term copy buffer (128 bytes)
    //   sp+368..375 = saved x29
    //   sp+376..383 = saved x30
    //
    // Callee-saved register usage:
    //   x19 = fmt_ptr (current position in format string)
    //   x20 = fmt_remaining_len
    //   x21 = arg_index
    //   x22 = args_base pointer (points to pushed args from caller)
    //   x23 = dest pointer (current write position in concat_buf)
    //   x24 = result_start pointer (beginning of result in concat_buf)
    //   x25 = concat_off pointer
    //   x26 = arg_count

    emitter.instruction("sub sp, sp, #384");                                    // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #368]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #368");                                   // set frame pointer

    // -- save callee-saved registers --
    emitter.instruction("stp x19, x20, [sp, #16]");                             // save x19, x20
    emitter.instruction("stp x21, x22, [sp, #32]");                             // save x21, x22
    emitter.instruction("stp x23, x24, [sp, #48]");                             // save x23, x24
    emitter.instruction("stp x25, x26, [sp, #64]");                             // save x25, x26

    // -- initialize state in callee-saved registers --
    emitter.instruction("mov x19, x1");                                         // fmt_ptr
    emitter.instruction("mov x20, x2");                                         // fmt_remaining_len
    emitter.instruction("mov x26, x0");                                         // arg_count
    emitter.instruction("mov x21, #0");                                         // arg_index = 0
    emitter.instruction("add x22, sp, #384");                                   // args_base (past our frame)

    // -- set up concat_buf destination --
    emitter.instruction("adrp x25, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x25, x25, _concat_off@PAGEOFF");                   // resolve concat_off address
    emitter.instruction("ldr x8, [x25]");                                       // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve buffer address
    emitter.instruction("add x23, x7, x8");                                     // dest pointer = buf + offset
    emitter.instruction("mov x24, x23");                                        // save result start

    // -- main format scanning loop --
    emitter.label("__rt_sprintf_loop");
    emitter.instruction("cbz x20, __rt_sprintf_done");                          // no format chars left
    emitter.instruction("ldrb w12, [x19], #1");                                 // load format char, advance
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining
    emitter.instruction("cmp w12, #37");                                        // is it '%'?
    emitter.instruction("b.eq __rt_sprintf_fmt");                               // yes → process format specifier

    // -- literal char: copy to output --
    emitter.instruction("strb w12, [x23], #1");                                 // copy literal char to output
    emitter.instruction("b __rt_sprintf_loop");                                 // next char

    // -- process format specifier --
    emitter.label("__rt_sprintf_fmt");
    emitter.instruction("cbz x20, __rt_sprintf_done");                          // no char after % → done
    emitter.instruction("ldrb w12, [x19]");                                     // peek at next char

    // -- %% → literal % --
    emitter.instruction("cmp w12, #37");                                        // is it '%'?
    emitter.instruction("b.ne __rt_sprintf_scan_spec");                         // no → scan full specifier
    emitter.instruction("add x19, x19, #1");                                    // consume the second '%'
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining
    emitter.instruction("strb w12, [x23], #1");                                 // write literal '%' to output
    emitter.instruction("b __rt_sprintf_loop");                                 // next

    // -- scan format specifier into mini buffer at sp+80 --
    // Build: '%' + [flags] + [width] + [.precision] + [ll] + type_char + '\0'
    emitter.label("__rt_sprintf_scan_spec");
    emitter.instruction("add x10, sp, #80");                                    // mini format buffer start
    emitter.instruction("mov w15, #37");                                        // '%' character
    emitter.instruction("strb w15, [x10], #1");                                 // write '%' to mini buffer

    // -- scan flags: '-', '+', '0', ' ', '#' --
    emitter.label("__rt_sprintf_scan_flags");
    emitter.instruction("cbz x20, __rt_sprintf_end_spec");                      // no chars left
    emitter.instruction("ldrb w12, [x19]");                                     // peek at current char
    emitter.instruction("cmp w12, #45");                                        // '-' flag?
    emitter.instruction("b.eq __rt_sprintf_copy_flag");                         // yes → copy it
    emitter.instruction("cmp w12, #43");                                        // '+' flag?
    emitter.instruction("b.eq __rt_sprintf_copy_flag");                         // yes → copy it
    emitter.instruction("cmp w12, #48");                                        // '0' flag?
    emitter.instruction("b.eq __rt_sprintf_copy_flag");                         // yes → copy it
    emitter.instruction("cmp w12, #32");                                        // ' ' flag?
    emitter.instruction("b.eq __rt_sprintf_copy_flag");                         // yes → copy it
    emitter.instruction("cmp w12, #35");                                        // '#' flag?
    emitter.instruction("b.eq __rt_sprintf_copy_flag");                         // yes → copy it
    emitter.instruction("b __rt_sprintf_scan_width");                           // no flag → try width

    emitter.label("__rt_sprintf_copy_flag");
    emitter.instruction("strb w12, [x10], #1");                                 // copy flag char to mini buffer
    emitter.instruction("add x19, x19, #1");                                    // consume char from format
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining
    emitter.instruction("b __rt_sprintf_scan_flags");                           // check for more flags

    // -- scan width: digits --
    emitter.label("__rt_sprintf_scan_width");
    emitter.instruction("cbz x20, __rt_sprintf_end_spec");                      // no chars left
    emitter.instruction("ldrb w12, [x19]");                                     // peek at current char
    emitter.instruction("cmp w12, #48");                                        // < '0'?
    emitter.instruction("b.lt __rt_sprintf_scan_dot");                          // yes → try precision dot
    emitter.instruction("cmp w12, #57");                                        // > '9'?
    emitter.instruction("b.gt __rt_sprintf_scan_dot");                          // yes → try precision dot
    emitter.instruction("strb w12, [x10], #1");                                 // copy width digit to mini buffer
    emitter.instruction("add x19, x19, #1");                                    // consume char
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining
    emitter.instruction("b __rt_sprintf_scan_width");                           // check for more digits

    // -- scan precision: '.' followed by digits --
    emitter.label("__rt_sprintf_scan_dot");
    emitter.instruction("cmp w12, #46");                                        // '.' ?
    emitter.instruction("b.ne __rt_sprintf_scan_type");                         // no → must be type char
    emitter.instruction("strb w12, [x10], #1");                                 // copy '.' to mini buffer
    emitter.instruction("add x19, x19, #1");                                    // consume '.'
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining

    emitter.label("__rt_sprintf_scan_prec");
    emitter.instruction("cbz x20, __rt_sprintf_end_spec");                      // no chars left
    emitter.instruction("ldrb w12, [x19]");                                     // peek at current char
    emitter.instruction("cmp w12, #48");                                        // < '0'?
    emitter.instruction("b.lt __rt_sprintf_scan_type");                         // no → type char
    emitter.instruction("cmp w12, #57");                                        // > '9'?
    emitter.instruction("b.gt __rt_sprintf_scan_type");                         // no → type char
    emitter.instruction("strb w12, [x10], #1");                                 // copy precision digit
    emitter.instruction("add x19, x19, #1");                                    // consume char
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining
    emitter.instruction("b __rt_sprintf_scan_prec");                            // check for more digits

    // -- read type character --
    emitter.label("__rt_sprintf_scan_type");
    emitter.instruction("cbz x20, __rt_sprintf_end_spec");                      // no chars left
    emitter.instruction("ldrb w12, [x19], #1");                                 // load type char, consume it
    emitter.instruction("sub x20, x20, #1");                                    // decrement remaining

    // Dispatch by type character
    emitter.instruction("cmp w12, #102");                                       // 'f' ?
    emitter.instruction("b.eq __rt_sprintf_type_float");                        // yes → float
    emitter.instruction("cmp w12, #101");                                       // 'e' ?
    emitter.instruction("b.eq __rt_sprintf_type_float");                        // yes → float
    emitter.instruction("cmp w12, #103");                                       // 'g' ?
    emitter.instruction("b.eq __rt_sprintf_type_float");                        // yes → float
    emitter.instruction("cmp w12, #115");                                       // 's' ?
    emitter.instruction("b.eq __rt_sprintf_type_str");                          // yes → string
    emitter.instruction("b __rt_sprintf_type_int");                             // default → integer

    // -- incomplete specifier at end of format string --
    emitter.label("__rt_sprintf_end_spec");
    emitter.instruction("b __rt_sprintf_done");                                 // bail out

    // ================================================================
    // FLOAT: %f, %e, %g (with optional flags/width/precision)
    // Passes the double value on the stack at [sp] for variadic ABI.
    // ================================================================
    emitter.label("__rt_sprintf_type_float");
    emitter.instruction("strb w12, [x10], #1");                                 // copy type char to mini buffer
    emitter.instruction("strb wzr, [x10]");                                     // null-terminate format string

    // -- load next arg (float bits) --
    emitter.instruction("lsl x15, x21, #4");                                    // arg offset = index * 16
    emitter.instruction("add x15, x22, x15");                                   // arg address in caller's stack
    emitter.instruction("ldr x3, [x15]");                                       // load float bits as integer
    emitter.instruction("add x21, x21, #1");                                    // increment arg index

    // -- store variadic arg on stack for snprintf --
    emitter.instruction("str x3, [sp]");                                        // variadic float bits at [sp]

    // -- call snprintf(buf, 128, fmt) with variadic float on stack --
    emitter.instruction("add x0, sp, #112");                                    // output buffer at sp+112
    emitter.instruction("mov x1, #128");                                        // buffer size
    emitter.instruction("add x2, sp, #80");                                     // mini format string at sp+80
    emitter.instruction("bl _snprintf");                                        // call libc snprintf
    // x0 = number of chars written

    // -- copy snprintf result to concat_buf --
    emitter.instruction("mov x4, x0");                                          // chars to copy
    emitter.instruction("add x3, sp, #112");                                    // source buffer

    emitter.label("__rt_sprintf_copy_f");
    emitter.instruction("cbz x4, __rt_sprintf_copy_f_done");                    // no bytes left
    emitter.instruction("ldrb w15, [x3], #1");                                  // load byte from snprintf output
    emitter.instruction("strb w15, [x23], #1");                                 // write to concat_buf
    emitter.instruction("sub x4, x4, #1");                                      // decrement counter
    emitter.instruction("b __rt_sprintf_copy_f");                               // continue copying

    emitter.label("__rt_sprintf_copy_f_done");
    emitter.instruction("b __rt_sprintf_loop");                                 // next format char

    // ================================================================
    // INTEGER: %d, %x, %o, %c, etc. (with optional flags/width/precision)
    // Uses %lld/%llx/%llo for 64-bit ints (except %c which stays 32-bit).
    // Passes the integer value on the stack at [sp] for variadic ABI.
    // ================================================================
    emitter.label("__rt_sprintf_type_int");

    // For 'd', 'x', 'o' we need 'll' prefix for 64-bit; 'c' stays as-is
    emitter.instruction("cmp w12, #99");                                        // 'c' ?
    emitter.instruction("b.eq __rt_sprintf_int_noprefix");                      // skip 'll' for %c

    // Write 'll' length modifier for 64-bit integer types
    emitter.instruction("mov w15, #108");                                       // 'l' character
    emitter.instruction("strb w15, [x10], #1");                                 // write first 'l' to mini buffer
    emitter.instruction("strb w15, [x10], #1");                                 // write second 'l' to mini buffer

    emitter.label("__rt_sprintf_int_noprefix");
    emitter.instruction("strb w12, [x10], #1");                                 // copy type char to mini buffer
    emitter.instruction("strb wzr, [x10]");                                     // null-terminate format string

    // -- load next arg (int value) --
    emitter.instruction("lsl x15, x21, #4");                                    // arg offset = index * 16
    emitter.instruction("add x15, x22, x15");                                   // arg address in caller's stack
    emitter.instruction("ldr x3, [x15]");                                       // load integer value
    emitter.instruction("add x21, x21, #1");                                    // increment arg index

    // -- store variadic arg on stack for snprintf --
    emitter.instruction("str x3, [sp]");                                        // variadic int at [sp]

    // -- call snprintf(buf, 128, fmt) with variadic int on stack --
    emitter.instruction("add x0, sp, #112");                                    // output buffer at sp+112
    emitter.instruction("mov x1, #128");                                        // buffer size
    emitter.instruction("add x2, sp, #80");                                     // mini format string at sp+80
    emitter.instruction("bl _snprintf");                                        // call libc snprintf
    // x0 = number of chars written

    // -- copy snprintf result to concat_buf --
    emitter.instruction("mov x4, x0");                                          // chars to copy
    emitter.instruction("add x3, sp, #112");                                    // source buffer

    emitter.label("__rt_sprintf_copy_i");
    emitter.instruction("cbz x4, __rt_sprintf_copy_i_done");                    // no bytes left
    emitter.instruction("ldrb w15, [x3], #1");                                  // load byte from snprintf output
    emitter.instruction("strb w15, [x23], #1");                                 // write to concat_buf
    emitter.instruction("sub x4, x4, #1");                                      // decrement counter
    emitter.instruction("b __rt_sprintf_copy_i");                               // continue copying

    emitter.label("__rt_sprintf_copy_i_done");
    emitter.instruction("b __rt_sprintf_loop");                                 // next format char

    // ================================================================
    // STRING: %s (with optional width/padding)
    // snprintf needs a null-terminated C string. Our strings are ptr+len,
    // so we copy the string to a temp buffer at sp+240 and null-terminate it.
    // The variadic pointer goes on the stack at [sp].
    // ================================================================
    emitter.label("__rt_sprintf_type_str");
    emitter.instruction("strb w12, [x10], #1");                                 // copy 's' to mini buffer
    emitter.instruction("strb wzr, [x10]");                                     // null-terminate format string

    // -- load next arg (string: ptr + tag|len) --
    emitter.instruction("lsl x15, x21, #4");                                    // arg offset = index * 16
    emitter.instruction("add x15, x22, x15");                                   // arg address in caller's stack
    emitter.instruction("ldr x3, [x15]");                                       // load string pointer
    emitter.instruction("ldr x4, [x15, #8]");                                   // load tag|length word
    emitter.instruction("lsr x4, x4, #8");                                      // extract length (shift right 8)
    emitter.instruction("add x21, x21, #1");                                    // increment arg index

    // -- copy string to temp buffer at sp+240 and null-terminate --
    // Limit copy to 127 bytes to fit in our 128-byte buffer
    emitter.instruction("cmp x4, #127");                                        // string longer than buffer?
    emitter.instruction("b.le __rt_sprintf_str_len_ok");                        // no → use actual length
    emitter.instruction("mov x4, #127");                                        // clamp to 127 bytes

    emitter.label("__rt_sprintf_str_len_ok");
    emitter.instruction("add x6, sp, #240");                                    // temp buffer for null-terminated copy
    emitter.instruction("mov x7, x4");                                          // bytes to copy

    emitter.label("__rt_sprintf_strcopy");
    emitter.instruction("cbz x7, __rt_sprintf_strcopy_done");                   // done copying
    emitter.instruction("ldrb w15, [x3], #1");                                  // load source byte
    emitter.instruction("strb w15, [x6], #1");                                  // write to temp buffer
    emitter.instruction("sub x7, x7, #1");                                      // decrement counter
    emitter.instruction("b __rt_sprintf_strcopy");                              // continue copying

    emitter.label("__rt_sprintf_strcopy_done");
    emitter.instruction("strb wzr, [x6]");                                      // null-terminate the copy

    // -- store variadic arg (pointer to null-terminated copy) on stack --
    emitter.instruction("add x3, sp, #240");                                    // pointer to null-terminated string
    emitter.instruction("str x3, [sp]");                                        // variadic string ptr at [sp]

    // -- call snprintf(buf, 128, fmt) with variadic string ptr on stack --
    emitter.instruction("add x0, sp, #112");                                    // output buffer at sp+112
    emitter.instruction("mov x1, #128");                                        // buffer size
    emitter.instruction("add x2, sp, #80");                                     // mini format string at sp+80
    emitter.instruction("bl _snprintf");                                        // call libc snprintf
    // x0 = number of chars written

    // -- copy snprintf result to concat_buf --
    emitter.instruction("mov x4, x0");                                          // chars to copy
    emitter.instruction("add x3, sp, #112");                                    // source buffer

    emitter.label("__rt_sprintf_copy_s");
    emitter.instruction("cbz x4, __rt_sprintf_copy_s_done");                    // no bytes left
    emitter.instruction("ldrb w15, [x3], #1");                                  // load byte from snprintf output
    emitter.instruction("strb w15, [x23], #1");                                 // write to concat_buf
    emitter.instruction("sub x4, x4, #1");                                      // decrement counter
    emitter.instruction("b __rt_sprintf_copy_s");                               // continue copying

    emitter.label("__rt_sprintf_copy_s_done");
    emitter.instruction("b __rt_sprintf_loop");                                 // next format char

    // ================================================================
    // DONE: finalize result and clean up
    // ================================================================
    emitter.label("__rt_sprintf_done");
    emitter.instruction("mov x1, x24");                                         // result start ptr in concat_buf
    emitter.instruction("sub x2, x23, x24");                                    // result length

    // -- update concat_off --
    emitter.instruction("ldr x8, [x25]");                                       // current concat offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x25]");                                       // store updated offset

    // -- prepare to pop args from caller's stack --
    emitter.instruction("mov x0, x26");                                         // arg_count
    emitter.instruction("lsl x0, x0, #4");                                      // bytes = count * 16

    // -- restore callee-saved registers --
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore x19, x20
    emitter.instruction("ldp x21, x22, [sp, #32]");                             // restore x21, x22
    emitter.instruction("ldp x23, x24, [sp, #48]");                             // restore x23, x24
    emitter.instruction("ldp x25, x26, [sp, #64]");                             // restore x25, x26
    emitter.instruction("ldp x29, x30, [sp, #368]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #384");                                    // deallocate our frame
    emitter.instruction("add sp, sp, x0");                                      // pop caller's args from stack
    emitter.instruction("ret");                                                 // return
}
