//! Purpose:
//! Emits the `__rt_sprintf`, `__rt_sprintf_loop_linux_x86_64` runtime helper assembly for Linux x86_64 sprintf formatting.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Formatting helpers parse format strings and marshal values through target ABI calls or emitted formatting paths.

use crate::codegen::emit::Emitter;

/// Emits the `__rt_sprintf` runtime helper for Linux x86_64.
///
/// ## Register contract on entry
/// - `rax`: pointer to the current format string
/// - `rdx`: remaining format string length in bytes
/// - `rdi`: packed variadic argument count
/// - `rsi`: pointer to caller-owned packed variadic argument records on the stack
///
/// ## Register contract on exit
/// - `rax`: pointer to the formatted string within the concat buffer
/// - `rdx`: length of the formatted string in bytes
///
/// ## Operation
/// Scans the format string byte-by-byte. Literal bytes are copied directly to the concat
/// buffer. `'%'` introduces a specifier: flags, width, precision, and type are parsed into a
/// local mini format string, then `snprintf` is invoked to format one argument. The result is
/// copied into the concat buffer and the format scan resumes.
///
/// Supported type characters: `%f`, `%e`, `%g` (float via `xmm0`), `%s` (string), `%c` (char),
/// and integer-like types via `snprintf`. `%%` emits a literal `'%'`.
///
/// The concat-buffer write cursor is advanced and published to the `_concat_off` symbol so
///串联 `printf` calls accumulate into a single output buffer. Callee-saved registers
/// (`rbx`, `r12`–`r15`, `rbp`) are preserved across the helper; `r11` and `rcx` are used as
/// temporaries. Caller-owned tagged variadic records are discarded before returning.
pub(super) fn emit_sprintf_linux_x86_64(emitter: &mut Emitter) {
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
