use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// number_format: format a float with decimals and custom separators.
/// Input:  d0 = number, x1 = decimals, x2 = dec_point char, x3 = thousands_sep char (0=none)
/// Output: x1 = pointer to string, x2 = length
/// Uses snprintf for decimal formatting, then inserts thousands separators.
pub fn emit_number_format(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_number_format_linux_x86_64(emitter);
        return;
    }

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
    emitter.bl_c("snprintf");                                        // call snprintf; returns char count in x0
    emitter.instruction("add sp, sp, #16");                                     // pop the variadic argument from stack
    emitter.instruction("str x0, [sp, #80]");                                   // save raw string length

    // -- set up concat_buf destination --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current concat_buf write offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
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

fn emit_number_format_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: number_format ---");
    emitter.label_global("__rt_number_format");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving local number_format() scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the raw snprintf buffer, mini format string, and concat-buffer state
    emitter.instruction("push rbx");                                            // preserve the concat-buffer destination cursor across the local formatting and copy loops
    emitter.instruction("push r12");                                            // preserve the concat-buffer start pointer for the final x86_64 string return pair
    emitter.instruction("push r13");                                            // preserve the concat-offset symbol address across the local formatting and copy loops
    emitter.instruction("sub rsp, 96");                                         // reserve local storage for the raw snprintf buffer, mini format string, and scalar formatting metadata
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // preserve the requested decimal count across the intermediate formatting and copy loops
    emitter.instruction("mov QWORD PTR [rbp - 48], rsi");                       // preserve the decimal-separator byte across the intermediate formatting and copy loops
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // preserve the thousands-separator byte across the intermediate formatting and copy loops
    emitter.instruction("mov BYTE PTR [rbp - 72], 37");                         // seed the mini format string with the leading '%' introducer
    emitter.instruction("mov BYTE PTR [rbp - 71], 46");                         // append the '.' precision introducer to the mini format string
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload the requested decimal count before converting it into the single supported ASCII precision digit
    emitter.instruction("add r8b, 48");                                         // convert the requested decimal count into its single-digit ASCII representation for the mini format string
    emitter.instruction("mov BYTE PTR [rbp - 70], r8b");                        // append the ASCII precision digit to the mini format string
    emitter.instruction("mov BYTE PTR [rbp - 69], 102");                        // append the trailing 'f' format type so snprintf renders a fixed-point decimal string
    emitter.instruction("mov BYTE PTR [rbp - 68], 0");                          // null-terminate the mini format string before handing it to snprintf
    emitter.instruction("lea rdi, [rbp - 120]");                                // point snprintf at the fixed local raw-decimal buffer that will be post-processed for thousands separators
    emitter.instruction("mov esi, 48");                                         // bound the raw-decimal buffer to 48 bytes before the variadic snprintf call
    emitter.instruction("lea rdx, [rbp - 72]");                                 // pass the mini format string to snprintf as the fixed-point format pointer
    emitter.instruction("mov eax, 1");                                          // advertise one live SIMD variadic register because the formatted number is passed in xmm0 on SysV x86_64
    emitter.bl_c("snprintf");                                                   // render the raw fixed-point decimal string into the local snprintf buffer
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // preserve the raw snprintf byte count before the thousands-separator pass consumes caller-saved registers
    crate::codegen::abi::emit_symbol_address(emitter, "r13", "_concat_off");
    emitter.instruction("mov r8, QWORD PTR [r13]");                             // load the current concat-buffer write cursor before appending the formatted output
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("lea rbx, [r9 + r8]");                                  // compute the concat-buffer destination cursor where the formatted output will begin
    emitter.instruction("mov r12, rbx");                                        // preserve the concat-buffer start pointer for the final x86_64 string return pair
    emitter.instruction("lea r10, [rbp - 120]");                                // point at the raw snprintf output buffer before scanning for a leading minus sign and decimal point
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload the raw snprintf byte count before splitting the integer and decimal parts
    emitter.instruction("movzx eax, BYTE PTR [r10]");                           // peek at the first raw formatted byte to detect a leading minus sign
    emitter.instruction("cmp al, 45");                                          // is the first raw formatted byte the leading '-' sign?
    emitter.instruction("jne __rt_nf_count_linux_x86_64");                      // skip the sign-copy fast path when the formatted number is non-negative
    emitter.instruction("mov BYTE PTR [rbx], al");                              // copy the leading minus sign into the concat buffer before processing the remaining digits
    emitter.instruction("add rbx, 1");                                          // advance the concat-buffer destination cursor after copying the leading minus sign
    emitter.instruction("add r10, 1");                                          // advance the raw formatted cursor past the copied leading minus sign
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining raw formatted byte count after removing the leading minus sign

    emitter.label("__rt_nf_count_linux_x86_64");
    emitter.instruction("mov r11, r10");                                        // preserve the start of the integer digit run before scanning forward to the decimal point
    emitter.instruction("xor esi, esi");                                        // start counting integer digits from zero before the decimal-point scan
    emitter.label("__rt_nf_count_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // have all remaining raw formatted bytes already been scanned for the decimal point?
    emitter.instruction("jz __rt_nf_count_done_linux_x86_64");                  // stop scanning once the raw formatted string has been fully consumed
    emitter.instruction("movzx eax, BYTE PTR [r10 + rsi]");                     // load the next raw formatted byte from the candidate integer-digit run
    emitter.instruction("cmp al, 46");                                          // is the current raw formatted byte the '.' decimal-point separator from snprintf?
    emitter.instruction("je __rt_nf_count_done_linux_x86_64");                  // stop counting integer digits once the decimal-point separator is reached
    emitter.instruction("add rsi, 1");                                          // count one more integer digit before continuing the decimal-point scan
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining raw formatted byte count after consuming one integer digit
    emitter.instruction("jmp __rt_nf_count_loop_linux_x86_64");                 // continue scanning the integer digit run until the decimal point or end of string is reached

    emitter.label("__rt_nf_count_done_linux_x86_64");
    emitter.instruction("xor edi, edi");                                        // start copying integer digits from logical index zero before inserting thousands separators
    emitter.instruction("mov rax, rsi");                                        // copy the integer-digit count into the dividend register before computing the leading group width
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before dividing the integer-digit count by the fixed group size
    emitter.instruction("mov r9d, 3");                                          // load the fixed thousands-group width into a scratch divisor register without clobbering the remaining-byte counter
    emitter.instruction("div r9");                                              // divide the integer-digit count by three so the remainder becomes the leading-group width
    emitter.instruction("mov r8, rdx");                                         // preserve the leading-group width remainder before the integer-copy loop mutates general-purpose registers
    emitter.instruction("test r8, r8");                                         // did the integer-digit count divide evenly into 3-digit groups?
    emitter.instruction("jnz __rt_nf_copy_int_linux_x86_64");                   // keep the remainder-derived leading-group width when the first group is shorter than three digits
    emitter.instruction("mov r8, 3");                                           // default the leading-group width to three digits when the integer-digit count is an exact multiple of three

    emitter.label("__rt_nf_copy_int_linux_x86_64");
    emitter.instruction("cmp rdi, rsi");                                        // have all integer digits already been copied into the concat buffer?
    emitter.instruction("jge __rt_nf_decimal_linux_x86_64");                    // move on to the decimal-part copy once the integer digit run has been fully emitted
    emitter.instruction("test rdi, rdi");                                       // is the current integer digit still part of the leading group?
    emitter.instruction("jz __rt_nf_no_sep_linux_x86_64");                      // skip separator insertion before the first emitted integer digit
    emitter.instruction("test r8, r8");                                         // has the current thousands group been exhausted exactly at this copy position?
    emitter.instruction("jnz __rt_nf_no_sep_linux_x86_64");                     // skip separator insertion until the current thousands group has been exhausted
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the configured thousands-separator byte before deciding whether to emit it
    emitter.instruction("test r9, r9");                                         // is thousands grouping disabled because the configured separator byte is zero?
    emitter.instruction("jz __rt_nf_no_sep_reset_linux_x86_64");                // skip emitting a separator when the caller requested no thousands separator
    emitter.instruction("mov BYTE PTR [rbx], r9b");                             // append the configured thousands-separator byte to the concat buffer
    emitter.instruction("add rbx, 1");                                          // advance the concat-buffer destination cursor after inserting one thousands separator byte
    emitter.label("__rt_nf_no_sep_reset_linux_x86_64");
    emitter.instruction("mov r8, 3");                                           // reset the remaining width of the next thousands group after crossing a group boundary

    emitter.label("__rt_nf_no_sep_linux_x86_64");
    emitter.instruction("movzx eax, BYTE PTR [r11 + rdi]");                     // load the next integer digit from the raw snprintf buffer
    emitter.instruction("mov BYTE PTR [rbx], al");                              // append the next integer digit to the concat buffer
    emitter.instruction("add rbx, 1");                                          // advance the concat-buffer destination cursor after copying one integer digit
    emitter.instruction("add rdi, 1");                                          // advance the logical integer-digit index after copying one integer digit
    emitter.instruction("sub r8, 1");                                           // consume one slot from the current thousands-group width after copying one integer digit
    emitter.instruction("jmp __rt_nf_copy_int_linux_x86_64");                   // continue copying integer digits until the full integer run has been emitted

    emitter.label("__rt_nf_decimal_linux_x86_64");
    emitter.instruction("add r11, rsi");                                        // advance the raw formatted cursor to the first decimal-part byte after the integer run
    emitter.label("__rt_nf_copy_dec_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // have all remaining decimal-part bytes already been copied into the concat buffer?
    emitter.instruction("jz __rt_nf_done_linux_x86_64");                        // finish once the full decimal part has been copied or omitted
    emitter.instruction("movzx eax, BYTE PTR [r11]");                           // load the next raw decimal-part byte before checking whether it is the snprintf decimal point
    emitter.instruction("add r11, 1");                                          // advance the raw formatted cursor after loading one decimal-part byte
    emitter.instruction("cmp al, 46");                                          // is the current decimal-part byte the '.' decimal-point separator emitted by snprintf?
    emitter.instruction("jne __rt_nf_store_dec_linux_x86_64");                  // copy non-decimal-point bytes directly into the concat buffer without translation
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the configured decimal-separator byte before replacing the snprintf decimal point
    emitter.instruction("mov eax, r9d");                                        // replace the raw snprintf decimal-point byte with the configured decimal-separator byte
    emitter.label("__rt_nf_store_dec_linux_x86_64");
    emitter.instruction("mov BYTE PTR [rbx], al");                              // append the current decimal-part byte to the concat buffer after any separator translation
    emitter.instruction("add rbx, 1");                                          // advance the concat-buffer destination cursor after copying one decimal-part byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining raw decimal-part byte count after copying one byte
    emitter.instruction("jmp __rt_nf_copy_dec_linux_x86_64");                   // continue copying the decimal part until every remaining raw byte has been emitted

    emitter.label("__rt_nf_done_linux_x86_64");
    emitter.instruction("mov rax, r12");                                        // return the concat-buffer start pointer of the formatted number in the primary x86_64 string result register
    emitter.instruction("mov rdx, rbx");                                        // copy the concat-buffer end cursor so the final formatted-string length can be derived
    emitter.instruction("sub rdx, rax");                                        // derive the formatted-string length from the concat-buffer start and end cursors
    emitter.instruction("mov r8, QWORD PTR [r13]");                             // reload the old concat-buffer write cursor before publishing the formatted-string append
    emitter.instruction("add r8, rdx");                                         // advance the concat-buffer write cursor by the emitted formatted-string length
    emitter.instruction("mov QWORD PTR [r13], r8");                             // publish the updated concat-buffer write cursor after appending the formatted number
    emitter.instruction("add rsp, 96");                                         // release the local raw-buffer and mini-format scratch space before restoring callee-saved registers
    emitter.instruction("pop r13");                                             // restore the saved concat-offset symbol register after the x86_64 number_format() helper finishes
    emitter.instruction("pop r12");                                             // restore the saved concat-buffer start register after the x86_64 number_format() helper finishes
    emitter.instruction("pop rbx");                                             // restore the saved concat-buffer destination cursor register after the x86_64 number_format() helper finishes
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the x86_64 formatted string pair
    emitter.instruction("ret");                                                 // return the formatted string pointer and length in the standard x86_64 string result registers
}
