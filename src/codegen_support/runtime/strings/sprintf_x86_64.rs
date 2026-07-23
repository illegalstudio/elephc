//! Purpose:
//! Emits the `__rt_sprintf`, `__rt_sprintf_loop_linux_x86_64` runtime helper assembly for Linux x86_64 sprintf formatting.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Formatting helpers parse format strings and marshal values through target ABI calls or emitted formatting paths.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Platform;

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
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current concat-buffer write cursor before appending the formatted output
    crate::codegen_support::abi::emit_symbol_address(emitter, "rcx", "_concat_buf");
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
    emitter.instruction("cmp r8b, 69");                                         // is the terminal type character '%E'?
    emitter.instruction("je __rt_sprintf_call_float_linux_x86_64");             // dispatch to the float snprintf path when the terminal type character is '%E'
    emitter.instruction("cmp r8b, 71");                                         // is the terminal type character '%G'?
    emitter.instruction("je __rt_sprintf_call_float_linux_x86_64");             // dispatch to the float snprintf path when the terminal type character is '%G'
    emitter.instruction("cmp r8b, 115");                                        // is the terminal type character '%s'?
    emitter.instruction("je __rt_sprintf_call_string_linux_x86_64");            // dispatch to the string snprintf path when the terminal type character is '%s'
    emitter.instruction("cmp r8b, 99");                                         // is the terminal type character '%c'?
    emitter.instruction("je __rt_sprintf_call_char_linux_x86_64");              // dispatch to the char snprintf path when the terminal type character is '%c'
    emitter.instruction("jmp __rt_sprintf_call_int_linux_x86_64");              // treat all remaining supported type characters as integer-like snprintf operands

    emitter.label("__rt_sprintf_end_spec_linux_x86_64");
    emitter.instruction("jmp __rt_sprintf_done_linux_x86_64");                  // stop formatting when the format string ends partway through a specifier

    emitter.label("__rt_sprintf_call_float_linux_x86_64");
    emitter.instruction("movq xmm0, QWORD PTR [r9]");                           // load the packed floating-point bits into xmm0 for the variadic snprintf call
    emitter.instruction("lea rdi, [rbp - 224]");                                // point snprintf at the fixed local output scratch buffer
    emitter.instruction("mov esi, 128");                                        // bound the local snprintf output scratch buffer to 128 bytes
    emitter.instruction("lea rdx, [rbp - 96]");                                 // pass the one-specifier mini format string to snprintf as the format pointer
    if emitter.platform == Platform::Windows {
        // This call has NO leading integer precision argument (the user's mini
        // format string already embeds precision/width/flags), so the double is
        // MSx64 positional argument 4 — `__rt_sys_snprintf` (register-shaped for
        // __rt_ftoa's `snprintf(buf, size, "%.*e", precision, double)`, where the
        // double is positional argument 5) would misroute it; call the
        // dedicated `__rt_sys_snprintf_double` shim instead (WF10b BUG A fix).
        emitter.instruction("call __rt_sys_snprintf_double");                   // route through the 3-fixed-arg + 1-variadic-double MSx64 shim
    } else {
        emitter.instruction("mov eax, 1");                                      // advertise one live SIMD variadic register to the SysV variadic call ABI
        emitter.emit_call_c("snprintf");                                        // format the floating operand into the local snprintf output scratch buffer
    }
    // -- PHP parity: %e/%E (and any exponential-form %g/%G) exponent uses the
    // -- minimum digit count (no leading zero), but CRT snprintf pads to at
    // -- least 2 digits. A double's decimal exponent never exceeds 3 digits and
    // -- 3-digit exponents are never zero-padded (they start at magnitude 100),
    // -- so the only possible padding is a single leading '0' in a 2-digit
    // -- exponent; strip it in place and shrink the byte count by one.
    emitter.instruction("lea rsi, [rbp - 224]");                                // scan cursor over the freshly formatted snprintf output
    emitter.instruction("mov rcx, rax");                                        // remaining bytes to scan for the 'e'/'E' exponent marker
    emitter.label("__rt_sprintf_etrim_scan_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // scanned the whole output without finding an exponent marker?
    emitter.instruction("jz __rt_sprintf_etrim_done_linux_x86_64");             // no exponent (e.g. %f) -> nothing to trim
    emitter.instruction("movzx edx, BYTE PTR [rsi]");                           // load the next output byte
    emitter.instruction("cmp dl, 101");                                         // is it 'e'?
    emitter.instruction("je __rt_sprintf_etrim_found_linux_x86_64");            // found the exponent marker
    emitter.instruction("cmp dl, 69");                                          // is it 'E'?
    emitter.instruction("je __rt_sprintf_etrim_found_linux_x86_64");            // found the exponent marker
    emitter.instruction("inc rsi");                                             // advance the scan cursor
    emitter.instruction("dec rcx");                                             // decrement the remaining scan length
    emitter.instruction("jmp __rt_sprintf_etrim_scan_linux_x86_64");            // keep scanning for the exponent marker
    emitter.label("__rt_sprintf_etrim_found_linux_x86_64");
    emitter.instruction("inc rsi");                                             // advance past the 'e'/'E' marker
    emitter.instruction("dec rcx");                                             // decrement the remaining scan length
    emitter.instruction("test rcx, rcx");                                       // malformed: exponent marker was the last byte?
    emitter.instruction("jz __rt_sprintf_etrim_done_linux_x86_64");             // bail out defensively
    emitter.instruction("movzx edx, BYTE PTR [rsi]");                           // load the byte after the exponent marker
    emitter.instruction("cmp dl, 43");                                          // is it '+'?
    emitter.instruction("je __rt_sprintf_etrim_sign_linux_x86_64");             // consume the exponent sign
    emitter.instruction("cmp dl, 45");                                          // is it '-'?
    emitter.instruction("jne __rt_sprintf_etrim_done_linux_x86_64");            // C99 always emits an exponent sign; bail defensively if absent
    emitter.label("__rt_sprintf_etrim_sign_linux_x86_64");
    emitter.instruction("inc rsi");                                             // advance past the exponent sign
    emitter.instruction("dec rcx");                                             // decrement the remaining scan length
    emitter.instruction("cmp rcx, 2");                                          // need at least two remaining bytes to test "0<digit>"
    emitter.instruction("jl __rt_sprintf_etrim_done_linux_x86_64");             // too short to be a padded 2-digit exponent
    emitter.instruction("movzx edx, BYTE PTR [rsi]");                           // load the first exponent digit
    emitter.instruction("cmp dl, 48");                                          // is it '0'?
    emitter.instruction("jne __rt_sprintf_etrim_done_linux_x86_64");            // not zero-padded -> nothing to strip
    emitter.instruction("movzx edx, BYTE PTR [rsi + 1]");                       // load the next byte after the leading zero
    emitter.instruction("cmp dl, 48");                                          // is it below '0'?
    emitter.instruction("jl __rt_sprintf_etrim_done_linux_x86_64");             // not a digit -> the '0' was the only exponent digit, keep it
    emitter.instruction("cmp dl, 57");                                          // is it above '9'?
    emitter.instruction("jg __rt_sprintf_etrim_done_linux_x86_64");             // not a digit -> keep the only exponent digit
    // -- guard: a right-justified WIDTH field pads BEFORE the sign/mantissa with
    // -- ' ' or '0'. Stripping a byte from the exponent would shrink the total
    // -- field width, so detect that padding and skip the strip entirely rather
    // -- than corrupt the requested width (a documented, bounded residual gap;
    // -- the no-width and left-justified-width cases below are fully handled).
    emitter.instruction("lea r8, [rbp - 224]");                                 // buffer start
    emitter.instruction("movzx edx, BYTE PTR [r8]");                            // first output byte
    emitter.instruction("cmp dl, 32");                                          // is it a space (space-padded field)?
    emitter.instruction("je __rt_sprintf_etrim_done_linux_x86_64");             // space padding present -> skip the strip
    emitter.instruction("mov r10, r8");                                         // cursor for the (optional) leading '-'/'+' sign
    emitter.instruction("cmp dl, 45");                                          // is the first byte a '-' sign?
    emitter.instruction("je __rt_sprintf_etrim_lead_sign_linux_x86_64");        // yes -> skip past it before checking for zero-padding
    emitter.instruction("cmp dl, 43");                                          // is the first byte a '+' sign (the '+' flag)?
    emitter.instruction("jne __rt_sprintf_etrim_lead_check_linux_x86_64");      // no sign -> check directly
    emitter.label("__rt_sprintf_etrim_lead_sign_linux_x86_64");
    emitter.instruction("inc r10");                                             // skip past the sign before checking for zero-padding
    emitter.label("__rt_sprintf_etrim_lead_check_linux_x86_64");
    emitter.instruction("movzx edx, BYTE PTR [r10]");                           // byte after the optional sign
    emitter.instruction("cmp dl, 48");                                          // is it '0'?
    emitter.instruction("jne __rt_sprintf_etrim_shift_setup_linux_x86_64");     // not zero -> no zero-padding, safe to strip
    emitter.instruction("movzx edx, BYTE PTR [r10 + 1]");                       // byte after that leading zero
    emitter.instruction("cmp dl, 46");                                          // is it '.' (the leading zero IS the legitimate mantissa digit)?
    emitter.instruction("je __rt_sprintf_etrim_shift_setup_linux_x86_64");      // legitimate "0.xxx" mantissa -> safe to strip the exponent
    emitter.instruction("jmp __rt_sprintf_etrim_done_linux_x86_64");            // zero-padded width field -> skip the strip
    // -- confirmed a padded 2-digit exponent ("0" + digit) with no right-justify padding: shift the tail left by one byte, dropping the leading zero --
    emitter.label("__rt_sprintf_etrim_shift_setup_linux_x86_64");
    emitter.instruction("lea r8, [rbp - 224]");                                 // recompute the scratch buffer base
    emitter.instruction("add r8, rax");                                         // r8 = original buffer end (using the pre-trim snprintf byte count)
    emitter.instruction("movzx edx, BYTE PTR [r8 - 1]");                        // last output byte (before this trim)
    emitter.instruction("cmp dl, 32");                                          // was the field left-justify space-padded?
    emitter.instruction("sete dil");                                            // dil = 1 when trailing-space padding is present
    emitter.instruction("movzx edi, dil");                                      // zero-extend the trailing-padding flag
    emitter.instruction("lea r9, [rsi + 1]");                                   // source cursor = byte after the leading zero
    emitter.instruction("mov r10, rsi");                                        // dest cursor = the leading zero's position
    emitter.label("__rt_sprintf_etrim_shift_linux_x86_64");
    emitter.instruction("cmp r9, r8");                                          // reached the end of the original output?
    emitter.instruction("jge __rt_sprintf_etrim_shift_done_linux_x86_64");      // shift complete
    emitter.instruction("movzx r11d, BYTE PTR [r9]");                           // load the next byte to shift down
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // shift it left by one position
    emitter.instruction("inc r9");                                              // advance the source cursor
    emitter.instruction("inc r10");                                             // advance the dest cursor
    emitter.instruction("jmp __rt_sprintf_etrim_shift_linux_x86_64");           // continue shifting
    emitter.label("__rt_sprintf_etrim_shift_done_linux_x86_64");
    emitter.instruction("test rdi, rdi");                                       // was trailing padding preserved?
    emitter.instruction("jz __rt_sprintf_etrim_shrink_linux_x86_64");           // no padding -> just shrink the byte count
    emitter.instruction("mov BYTE PTR [r10], 32");                              // restore the requested field width with one more trailing pad space
    emitter.instruction("jmp __rt_sprintf_etrim_done_linux_x86_64");            // keep rax unchanged: total field width is preserved
    emitter.label("__rt_sprintf_etrim_shrink_linux_x86_64");
    emitter.instruction("dec rax");                                             // no padding to preserve -> one byte shorter after dropping the leading exponent zero
    emitter.label("__rt_sprintf_etrim_done_linux_x86_64");
    emitter.instruction("jmp __rt_sprintf_copy_result_linux_x86_64");           // copy the freshly formatted (and exponent-trimmed) snprintf output into the concat buffer

    emitter.label("__rt_sprintf_call_int_linux_x86_64");
    emitter.instruction("lea rdi, [rbp - 224]");                                // point snprintf at the fixed local output scratch buffer
    emitter.instruction("mov esi, 128");                                        // bound the local snprintf output scratch buffer to 128 bytes
    emitter.instruction("lea rdx, [rbp - 96]");                                 // pass the one-specifier mini format string to snprintf as the format pointer
    emitter.instruction("mov rcx, QWORD PTR [r9]");                             // load the packed integer payload into the first SysV variadic integer register
    emitter.instruction("xor eax, eax");                                        // advertise that no SIMD variadic registers are live for the integer snprintf call
    emitter.emit_call_c("snprintf");                                            // format the integer-like operand into the local snprintf output scratch buffer
    emitter.instruction("jmp __rt_sprintf_copy_result_linux_x86_64");           // copy the freshly formatted snprintf output into the concat buffer

    emitter.label("__rt_sprintf_call_char_linux_x86_64");
    emitter.instruction("lea rdi, [rbp - 224]");                                // point snprintf at the fixed local output scratch buffer
    emitter.instruction("mov esi, 128");                                        // bound the local snprintf output scratch buffer to 128 bytes
    emitter.instruction("lea rdx, [rbp - 96]");                                 // pass the one-specifier mini format string to snprintf as the format pointer
    emitter.instruction("mov ecx, DWORD PTR [r9]");                             // load the packed character payload into the first SysV variadic integer register
    emitter.instruction("xor eax, eax");                                        // advertise that no SIMD variadic registers are live for the char snprintf call
    emitter.emit_call_c("snprintf");                                            // format the character operand into the local snprintf output scratch buffer
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
    emitter.emit_call_c("snprintf");                                            // format the string operand into the local snprintf output scratch buffer

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

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Regression test for the sprintf snprintf-call Class-1 ABI bug (WF2/F2): on
    /// windows-x86_64, `bl_c("snprintf")` reached the raw msvcrt import with
    /// SysV-staged arguments (rdi/rsi/rdx/rcx/xmm0) instead of the MSx64 ABI that
    /// msvcrt `snprintf` expects (rcx/rdx/r8/r9), corrupting every sprintf-formatted
    /// value. `emit_call_c("snprintf")` routes the call through the
    /// `__rt_sys_snprintf` shim instead, which performs the SysV->MSx64 conversion.
    #[test]
    fn test_emit_sprintf_windows_x86_64_routes_snprintf_through_shim() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_sprintf_linux_x86_64(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("call __rt_sys_snprintf\n"));
        assert!(!asm.contains("call snprintf\n"));
    }

    /// Companion non-Windows control: on Linux x86_64 (and every other non-Windows
    /// target), `emit_call_c` is byte-identical to `bl_c`, so the format-specifier
    /// dispatch paths must still emit a bare `call snprintf` unchanged.
    #[test]
    fn test_emit_sprintf_linux_x86_64_still_calls_bare_snprintf() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_sprintf_linux_x86_64(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("call snprintf\n"));
        assert!(!asm.contains("__rt_sys_snprintf"));
    }

    /// Regression test for WF10b BUG A ("windows garbage output"): the `__rt_sprintf`
    /// float path (`%f`/`%e`/`%g`/`%E`/`%G`) has NO leading integer precision
    /// argument — its `snprintf(buf, size, fmt, double)` call shape differs from
    /// `__rt_ftoa`'s `snprintf(buf, size, "%.*e", precision, double)` shape, so it
    /// must route through the dedicated `__rt_sys_snprintf_double` shim on
    /// windows-x86_64, NOT the generic `__rt_sys_snprintf` shim (which would stage
    /// the double at the wrong argument position and produce garbage).
    #[test]
    fn test_emit_sprintf_windows_x86_64_float_path_routes_through_snprintf_double_shim() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_sprintf_linux_x86_64(&mut emitter);
        let asm = emitter.output();

        let float_start = asm
            .find("__rt_sprintf_call_float_linux_x86_64:\n")
            .expect("float dispatch label missing");
        let float_end = asm[float_start..]
            .find("__rt_sprintf_call_int_linux_x86_64:\n")
            .map(|offset| float_start + offset)
            .expect("int dispatch label missing after float section");
        let float_section = &asm[float_start..float_end];

        assert!(
            float_section.contains("call __rt_sys_snprintf_double\n"),
            "the float path must call the dedicated double-arg4 shim"
        );
        assert!(
            !float_section.contains("call __rt_sys_snprintf\n"),
            "the float path must NOT reach the generic (precision-int-shaped) snprintf shim"
        );
    }

    /// Companion non-Windows control: the float path's SysV variadic staging
    /// (`mov eax, 1` then a bare `call snprintf`) must stay unchanged on Linux/macOS.
    #[test]
    fn test_emit_sprintf_linux_x86_64_float_path_still_calls_bare_snprintf() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_sprintf_linux_x86_64(&mut emitter);
        let asm = emitter.output();

        let float_start = asm
            .find("__rt_sprintf_call_float_linux_x86_64:\n")
            .expect("float dispatch label missing");
        let float_end = asm[float_start..]
            .find("__rt_sprintf_call_int_linux_x86_64:\n")
            .map(|offset| float_start + offset)
            .expect("int dispatch label missing after float section");
        let float_section = &asm[float_start..float_end];

        assert!(float_section.contains("mov eax, 1\n"));
        assert!(float_section.contains("call snprintf\n"));
        assert!(!float_section.contains("__rt_sys_snprintf_double"));
    }

    /// Verifies the `%E`/`%G` uppercase specifiers dispatch to the float path
    /// alongside `%f`/`%e`/`%g` (a WF10b fix: they previously fell through to the
    /// integer path, reinterpreting the double's raw bits as an integer and
    /// producing garbage — e.g. `sprintf("%E", 1.0)` read the mantissa bit pattern
    /// as an int64 instead of formatting the float).
    #[test]
    fn test_emit_sprintf_dispatches_uppercase_e_and_g_to_float_path() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_sprintf_linux_x86_64(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("cmp r8b, 69\n"), "'E' (69) must be checked");
        assert!(asm.contains("cmp r8b, 71\n"), "'G' (71) must be checked");
    }

    /// Verifies the `%e`/`%E` exponent-trim: PHP's minimum-digit exponent
    /// (`1.0e+4`, not CRT's `1.0e+04`) is produced by stripping a lone padded
    /// leading zero from a 2-digit CRT exponent, guarded against corrupting an
    /// explicit field WIDTH (see the padding-detection instructions before the
    /// shift loop).
    #[test]
    fn test_emit_sprintf_float_path_has_exponent_trim_with_padding_guard() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_sprintf_linux_x86_64(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_sprintf_etrim_scan_linux_x86_64:\n"));
        assert!(asm.contains("__rt_sprintf_etrim_shift_setup_linux_x86_64:\n"));
        assert!(
            asm.contains("cmp dl, 32\n"),
            "must detect space-padded (right-justified) fields to guard the strip"
        );
    }
}
