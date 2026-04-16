use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

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
    if emitter.target.arch == Arch::X86_64 {
        emit_sprintf_linux_x86_64(emitter);
        return;
    }

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
    crate::codegen::abi::emit_symbol_address(emitter, "x25", "_concat_off");
    emitter.instruction("ldr x8, [x25]");                                       // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
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
    emitter.bl_c("snprintf");                                        // call libc snprintf
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
    emitter.bl_c("snprintf");                                        // call libc snprintf
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
    emitter.bl_c("snprintf");                                        // call libc snprintf
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

fn emit_sprintf_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf ---");
    emitter.label_global("__rt_sprintf");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving sprintf() local storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the format cursor, variadic cursor, and scratch buffers
    emitter.instruction("push rbx");                                            // preserve the concat-buffer destination cursor across nested snprintf and cstr helper calls
    emitter.instruction("push r12");                                            // preserve the format-string pointer across nested helper calls
    emitter.instruction("push r13");                                            // preserve the remaining format-string length across nested helper calls
    emitter.instruction("push r14");                                            // preserve the packed variadic argument index across nested helper calls
    emitter.instruction("push r15");                                            // preserve the caller-stack variadic base pointer across nested helper calls
    emitter.instruction("sub rsp, 328");                                        // reserve aligned local storage for the mini format buffer, snprintf output buffer, and temporary C string copy
    emitter.instruction("mov r12, rax");                                        // preserve the current format-string pointer across the whole sprintf scan loop
    emitter.instruction("mov r13, rdx");                                        // preserve the remaining format-string length across the whole sprintf scan loop
    emitter.instruction("xor r14d, r14d");                                      // start consuming packed variadic argument records from logical index zero
    emitter.instruction("lea r15, [rbp + 16]");                                 // point at the caller-owned packed variadic argument records that begin above the saved return address
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // preserve the packed variadic argument count so the helper can discard the caller records before returning
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current concat-buffer write cursor before appending the formatted output
    crate::codegen::abi::emit_symbol_address(emitter, "rcx", "_concat_buf");
    emitter.instruction("lea rbx, [rcx + r11]");                                // compute the concat-buffer destination cursor where the formatted output will begin
    emitter.instruction("mov QWORD PTR [rbp - 48], rbx");                       // preserve the concat-buffer start pointer for the final x86_64 string return pair
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // preserve the concat-offset symbol address so the helper can publish the new write cursor

    emitter.label("__rt_sprintf_loop_linux_x86_64");
    emitter.instruction("test r13, r13");                                       // has the entire format string been consumed already?
    emitter.instruction("jz __rt_sprintf_done_linux_x86_64");                   // stop scanning once the format string is exhausted
    emitter.instruction("movzx r8d, BYTE PTR [r12]");                           // load the next format byte before deciding whether it is literal text or a format specifier
    emitter.instruction("add r12, 1");                                          // advance the format cursor after consuming one format byte
    emitter.instruction("sub r13, 1");                                          // decrement the remaining format length after consuming one format byte
    emitter.instruction("cmp r8b, 37");                                         // is the current format byte the '%' introducer of a format specifier?
    emitter.instruction("je __rt_sprintf_fmt_linux_x86_64");                    // branch into format-specifier parsing when the current format byte is '%'
    emitter.instruction("mov BYTE PTR [rbx], r8b");                             // copy the literal format byte directly into the concat-buffer destination cursor
    emitter.instruction("add rbx, 1");                                          // advance the concat-buffer destination cursor after copying one literal byte
    emitter.instruction("jmp __rt_sprintf_loop_linux_x86_64");                  // continue scanning the remaining format string after copying a literal byte

    emitter.label("__rt_sprintf_fmt_linux_x86_64");
    emitter.instruction("test r13, r13");                                       // is the format string exhausted immediately after the '%' introducer?
    emitter.instruction("jz __rt_sprintf_done_linux_x86_64");                   // stop scanning when a trailing '%' lacks a following type character
    emitter.instruction("movzx r8d, BYTE PTR [r12]");                           // peek at the next format byte before deciding between '%%' and a typed specifier
    emitter.instruction("cmp r8b, 37");                                         // is the current format sequence '%%' for a literal percent sign?
    emitter.instruction("jne __rt_sprintf_scan_spec_linux_x86_64");             // fall through to typed specifier scanning when the current format sequence is not '%%'
    emitter.instruction("add r12, 1");                                          // consume the second '%' byte after recognizing the literal percent escape
    emitter.instruction("sub r13, 1");                                          // decrement the remaining format length after consuming the literal percent escape
    emitter.instruction("mov BYTE PTR [rbx], r8b");                             // write the literal '%' byte into the concat-buffer destination cursor
    emitter.instruction("add rbx, 1");                                          // advance the concat-buffer destination cursor after writing the literal percent byte
    emitter.instruction("jmp __rt_sprintf_loop_linux_x86_64");                  // continue scanning after emitting the literal percent escape

    emitter.label("__rt_sprintf_scan_spec_linux_x86_64");
    emitter.instruction("lea r10, [rbp - 96]");                                 // point at the mini format-string buffer used for one-specifier snprintf calls
    emitter.instruction("mov BYTE PTR [r10], 37");                              // seed the mini format-string buffer with the leading '%' introducer
    emitter.instruction("add r10, 1");                                          // advance the mini format-string cursor after writing the leading '%' introducer

    emitter.label("__rt_sprintf_scan_flags_linux_x86_64");
    emitter.instruction("test r13, r13");                                       // are there any format bytes left to inspect for flag characters?
    emitter.instruction("jz __rt_sprintf_end_spec_linux_x86_64");               // bail out cleanly when the format string ends before a type character appears
    emitter.instruction("movzx r8d, BYTE PTR [r12]");                           // peek at the next format byte before deciding whether it is one of the allowed flag characters
    emitter.instruction("cmp r8b, 45");                                         // is the next format byte the left-align '-' flag?
    emitter.instruction("je __rt_sprintf_copy_flag_linux_x86_64");              // copy the current flag byte into the mini format string when it is '-'
    emitter.instruction("cmp r8b, 43");                                         // is the next format byte the explicit plus-sign '+' flag?
    emitter.instruction("je __rt_sprintf_copy_flag_linux_x86_64");              // copy the current flag byte into the mini format string when it is '+'
    emitter.instruction("cmp r8b, 48");                                         // is the next format byte the zero-pad '0' flag?
    emitter.instruction("je __rt_sprintf_copy_flag_linux_x86_64");              // copy the current flag byte into the mini format string when it is '0'
    emitter.instruction("cmp r8b, 32");                                         // is the next format byte the space-sign flag?
    emitter.instruction("je __rt_sprintf_copy_flag_linux_x86_64");              // copy the current flag byte into the mini format string when it is a space
    emitter.instruction("cmp r8b, 35");                                         // is the next format byte the alternate-form '#' flag?
    emitter.instruction("je __rt_sprintf_copy_flag_linux_x86_64");              // copy the current flag byte into the mini format string when it is '#'
    emitter.instruction("jmp __rt_sprintf_scan_width_linux_x86_64");            // move on to width parsing once no more flag characters remain

    emitter.label("__rt_sprintf_copy_flag_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r10], r8b");                             // append the current flag byte to the mini format string
    emitter.instruction("add r10, 1");                                          // advance the mini format-string cursor after appending one flag byte
    emitter.instruction("add r12, 1");                                          // consume the current flag byte from the source format string
    emitter.instruction("sub r13, 1");                                          // decrement the remaining format length after consuming one flag byte
    emitter.instruction("jmp __rt_sprintf_scan_flags_linux_x86_64");            // continue scanning for additional flag bytes

    emitter.label("__rt_sprintf_scan_width_linux_x86_64");
    emitter.instruction("test r13, r13");                                       // are there any format bytes left to inspect for width digits?
    emitter.instruction("jz __rt_sprintf_end_spec_linux_x86_64");               // bail out cleanly when the format string ends before a type character appears
    emitter.instruction("movzx r8d, BYTE PTR [r12]");                           // peek at the next format byte before deciding whether it is a width digit
    emitter.instruction("cmp r8b, 48");                                         // is the next format byte below ASCII '0'?
    emitter.instruction("jl __rt_sprintf_scan_dot_linux_x86_64");               // move on to precision parsing once the next byte is not a width digit
    emitter.instruction("cmp r8b, 57");                                         // is the next format byte above ASCII '9'?
    emitter.instruction("jg __rt_sprintf_scan_dot_linux_x86_64");               // move on to precision parsing once the next byte is not a width digit
    emitter.instruction("mov BYTE PTR [r10], r8b");                             // append the current width digit to the mini format string
    emitter.instruction("add r10, 1");                                          // advance the mini format-string cursor after appending one width digit
    emitter.instruction("add r12, 1");                                          // consume the current width digit from the source format string
    emitter.instruction("sub r13, 1");                                          // decrement the remaining format length after consuming one width digit
    emitter.instruction("jmp __rt_sprintf_scan_width_linux_x86_64");            // continue scanning for additional width digits

    emitter.label("__rt_sprintf_scan_dot_linux_x86_64");
    emitter.instruction("cmp r8b, 46");                                         // is the next format byte the '.' introducer of a precision clause?
    emitter.instruction("jne __rt_sprintf_scan_type_linux_x86_64");             // skip precision parsing when the next format byte is not '.'
    emitter.instruction("mov BYTE PTR [r10], r8b");                             // append the '.' precision introducer to the mini format string
    emitter.instruction("add r10, 1");                                          // advance the mini format-string cursor after appending the precision introducer
    emitter.instruction("add r12, 1");                                          // consume the precision introducer from the source format string
    emitter.instruction("sub r13, 1");                                          // decrement the remaining format length after consuming the precision introducer

    emitter.label("__rt_sprintf_scan_prec_linux_x86_64");
    emitter.instruction("test r13, r13");                                       // are there any format bytes left to inspect for precision digits?
    emitter.instruction("jz __rt_sprintf_end_spec_linux_x86_64");               // bail out cleanly when the format string ends before a type character appears
    emitter.instruction("movzx r8d, BYTE PTR [r12]");                           // peek at the next format byte before deciding whether it is a precision digit
    emitter.instruction("cmp r8b, 48");                                         // is the next format byte below ASCII '0'?
    emitter.instruction("jl __rt_sprintf_scan_type_linux_x86_64");              // move on to type parsing once the next byte is not a precision digit
    emitter.instruction("cmp r8b, 57");                                         // is the next format byte above ASCII '9'?
    emitter.instruction("jg __rt_sprintf_scan_type_linux_x86_64");              // move on to type parsing once the next byte is not a precision digit
    emitter.instruction("mov BYTE PTR [r10], r8b");                             // append the current precision digit to the mini format string
    emitter.instruction("add r10, 1");                                          // advance the mini format-string cursor after appending one precision digit
    emitter.instruction("add r12, 1");                                          // consume the current precision digit from the source format string
    emitter.instruction("sub r13, 1");                                          // decrement the remaining format length after consuming one precision digit
    emitter.instruction("jmp __rt_sprintf_scan_prec_linux_x86_64");             // continue scanning for additional precision digits

    emitter.label("__rt_sprintf_scan_type_linux_x86_64");
    emitter.instruction("test r13, r13");                                       // is the format string exhausted before a terminal type character appears?
    emitter.instruction("jz __rt_sprintf_end_spec_linux_x86_64");               // bail out cleanly when the format string ends before the type character
    emitter.instruction("movzx r8d, BYTE PTR [r12]");                           // load the terminal type character that completes the current mini format string
    emitter.instruction("add r12, 1");                                          // consume the type character from the source format string
    emitter.instruction("sub r13, 1");                                          // decrement the remaining format length after consuming the type character
    emitter.instruction("mov BYTE PTR [r10], r8b");                             // append the terminal type character to the mini format string
    emitter.instruction("add r10, 1");                                          // advance the mini format-string cursor after appending the type character
    emitter.instruction("mov BYTE PTR [r10], 0");                               // null-terminate the one-specifier mini format string for the upcoming snprintf call
    emitter.instruction("mov r9, r14");                                         // copy the packed variadic argument index before scaling it into a caller-stack record offset
    emitter.instruction("shl r9, 4");                                           // convert the packed variadic argument index into the byte offset of the current 16-byte caller-stack record
    emitter.instruction("lea r9, [r15 + r9]");                                  // compute the address of the current packed variadic argument record on the caller stack
    emitter.instruction("add r14, 1");                                          // consume one packed variadic argument record for the current format specifier
    emitter.instruction("cmp r8b, 102");                                        // is the terminal type character '%f'?
    emitter.instruction("je __rt_sprintf_call_float_linux_x86_64");             // dispatch to the float snprintf path when the terminal type character is '%f'
    emitter.instruction("cmp r8b, 101");                                        // is the terminal type character '%e'?
    emitter.instruction("je __rt_sprintf_call_float_linux_x86_64");             // dispatch to the float snprintf path when the terminal type character is '%e'
    emitter.instruction("cmp r8b, 103");                                        // is the terminal type character '%g'?
    emitter.instruction("je __rt_sprintf_call_float_linux_x86_64");             // dispatch to the float snprintf path when the terminal type character is '%g'
    emitter.instruction("cmp r8b, 115");                                        // is the terminal type character '%s'?
    emitter.instruction("je __rt_sprintf_call_string_linux_x86_64");            // dispatch to the string snprintf path when the terminal type character is '%s'
    emitter.instruction("cmp r8b, 99");                                         // is the terminal type character '%c'?
    emitter.instruction("je __rt_sprintf_call_char_linux_x86_64");              // dispatch to the char snprintf path when the terminal type character is '%c'
    emitter.instruction("jmp __rt_sprintf_call_int_linux_x86_64");              // treat all remaining supported type characters as integer-like snprintf operands

    emitter.label("__rt_sprintf_end_spec_linux_x86_64");
    emitter.instruction("jmp __rt_sprintf_done_linux_x86_64");                  // stop formatting when the format string ends partway through a specifier

    emitter.label("__rt_sprintf_call_float_linux_x86_64");
    emitter.instruction("movq xmm0, QWORD PTR [r9]");                           // load the packed floating-point bits into xmm0 for the SysV variadic snprintf call
    emitter.instruction("lea rdi, [rbp - 224]");                                // point snprintf at the fixed local output scratch buffer
    emitter.instruction("mov esi, 128");                                        // bound the local snprintf output scratch buffer to 128 bytes
    emitter.instruction("lea rdx, [rbp - 96]");                                 // pass the one-specifier mini format string to snprintf as the format pointer
    emitter.instruction("mov eax, 1");                                          // advertise one live SIMD variadic register to the SysV variadic call ABI
    emitter.bl_c("snprintf");                                                   // format the floating operand into the local snprintf output scratch buffer
    emitter.instruction("jmp __rt_sprintf_copy_result_linux_x86_64");           // copy the freshly formatted snprintf output into the concat buffer

    emitter.label("__rt_sprintf_call_int_linux_x86_64");
    emitter.instruction("lea rdi, [rbp - 224]");                                // point snprintf at the fixed local output scratch buffer
    emitter.instruction("mov esi, 128");                                        // bound the local snprintf output scratch buffer to 128 bytes
    emitter.instruction("lea rdx, [rbp - 96]");                                 // pass the one-specifier mini format string to snprintf as the format pointer
    emitter.instruction("mov rcx, QWORD PTR [r9]");                             // load the packed integer payload into the first SysV variadic integer register
    emitter.instruction("xor eax, eax");                                        // advertise that no SIMD variadic registers are live for the integer snprintf call
    emitter.bl_c("snprintf");                                                   // format the integer-like operand into the local snprintf output scratch buffer
    emitter.instruction("jmp __rt_sprintf_copy_result_linux_x86_64");           // copy the freshly formatted snprintf output into the concat buffer

    emitter.label("__rt_sprintf_call_char_linux_x86_64");
    emitter.instruction("lea rdi, [rbp - 224]");                                // point snprintf at the fixed local output scratch buffer
    emitter.instruction("mov esi, 128");                                        // bound the local snprintf output scratch buffer to 128 bytes
    emitter.instruction("lea rdx, [rbp - 96]");                                 // pass the one-specifier mini format string to snprintf as the format pointer
    emitter.instruction("mov ecx, DWORD PTR [r9]");                             // load the packed character payload into the first SysV variadic integer register
    emitter.instruction("xor eax, eax");                                        // advertise that no SIMD variadic registers are live for the char snprintf call
    emitter.bl_c("snprintf");                                                   // format the character operand into the local snprintf output scratch buffer
    emitter.instruction("jmp __rt_sprintf_copy_result_linux_x86_64");           // copy the freshly formatted snprintf output into the concat buffer

    emitter.label("__rt_sprintf_call_string_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the packed elephc string pointer before converting it into a null-terminated C string
    emitter.instruction("mov rdx, QWORD PTR [r9 + 8]");                         // load the packed string metadata word before extracting the elephc string length
    emitter.instruction("shr rdx, 8");                                          // extract the original elephc string length from the packed string metadata word
    emitter.instruction("call __rt_cstr");                                      // convert the elephc string pointer+length pair into a null-terminated C string in the scratch buffer
    emitter.instruction("lea rdi, [rbp - 224]");                                // point snprintf at the fixed local output scratch buffer
    emitter.instruction("mov esi, 128");                                        // bound the local snprintf output scratch buffer to 128 bytes
    emitter.instruction("lea rdx, [rbp - 96]");                                 // pass the one-specifier mini format string to snprintf as the format pointer
    emitter.instruction("mov rcx, rax");                                        // pass the null-terminated C string pointer in the first SysV variadic integer register
    emitter.instruction("xor eax, eax");                                        // advertise that no SIMD variadic registers are live for the string snprintf call
    emitter.bl_c("snprintf");                                                   // format the string operand into the local snprintf output scratch buffer

    emitter.label("__rt_sprintf_copy_result_linux_x86_64");
    emitter.instruction("mov rcx, rax");                                        // copy the snprintf-written byte count before the result-copy loop consumes caller-saved registers
    emitter.instruction("lea r10, [rbp - 224]");                                // point at the local snprintf output scratch buffer before copying its bytes into the concat buffer
    emitter.label("__rt_sprintf_copy_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // have all bytes produced by snprintf already been copied into the concat buffer?
    emitter.instruction("jz __rt_sprintf_loop_linux_x86_64");                   // resume scanning the source format string once the local snprintf output has been fully copied
    emitter.instruction("mov r11b, BYTE PTR [r10]");                            // load the next byte from the local snprintf output scratch buffer
    emitter.instruction("mov BYTE PTR [rbx], r11b");                            // store the current snprintf output byte into the concat-buffer destination cursor
    emitter.instruction("add r10, 1");                                          // advance the local snprintf output cursor after copying one byte
    emitter.instruction("add rbx, 1");                                          // advance the concat-buffer destination cursor after copying one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining snprintf output byte count after copying one byte
    emitter.instruction("jmp __rt_sprintf_copy_loop_linux_x86_64");             // continue copying snprintf output until every byte has been appended to the concat buffer

    emitter.label("__rt_sprintf_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the concat-buffer start pointer of the formatted string in the primary x86_64 string result register
    emitter.instruction("mov rdx, rbx");                                        // copy the concat-buffer end cursor so the final formatted-string length can be derived
    emitter.instruction("sub rdx, rax");                                        // derive the formatted-string length from the concat-buffer start/end pointers
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the concat-offset symbol address before publishing the new write cursor
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // reload the old concat-buffer write cursor before advancing it by the formatted-string length
    emitter.instruction("add r11, rdx");                                        // advance the concat-buffer write cursor by the emitted formatted-string length
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the updated concat-buffer write cursor after emitting the formatted string
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload the packed variadic argument count before discarding the caller-owned tagged argument records
    emitter.instruction("shl rcx, 4");                                          // convert the packed variadic argument count into the total tagged-record byte count on the caller stack
    emitter.instruction("add rsp, 328");                                        // release the local sprintf() buffers before restoring callee-saved registers
    emitter.instruction("pop r15");                                             // restore the caller packed-argument base-pointer callee-saved register
    emitter.instruction("pop r14");                                             // restore the caller packed-argument index callee-saved register
    emitter.instruction("pop r13");                                             // restore the caller format-length callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller format-pointer callee-saved register
    emitter.instruction("pop rbx");                                             // restore the caller concat-destination callee-saved register
    emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                        // preserve the dynamic return address before rethreading the stack past the caller-owned tagged argument records
    emitter.instruction("mov rbp, QWORD PTR [rsp]");                            // restore the caller frame pointer without consuming the current stack slot yet
    emitter.instruction("lea rsp, [rsp + rcx + 16]");                           // advance past the saved frame pointer, return address, and tagged variadic records to the caller post-call stack top
    emitter.instruction("push r11");                                            // recreate the preserved return address at the top of the rethreaded stack so a plain ret lands back in generated code
    emitter.instruction("ret");                                                 // return the formatted string in the standard x86_64 string result registers while also discarding the caller-owned tagged arguments
}
