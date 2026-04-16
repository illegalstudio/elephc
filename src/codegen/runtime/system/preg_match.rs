use crate::codegen::{emit::Emitter, platform::Arch};

/// __rt_preg_match: check if a POSIX regex matches a subject string.
/// Input:  x1=pattern ptr, x2=pattern len, x3=subject ptr, x4=subject len
/// Output: x0=1 if match found, 0 if not
pub(crate) fn emit_preg_match(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_match_linux_x86_64(emitter);
        return;
    }

    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let pattern_ptr_off = regmatch_off + emitter.platform.regmatch_t_size();
    let pattern_len_off = pattern_ptr_off + 8;
    let subject_ptr_off = pattern_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let regexec_result_off = subject_cstr_off + 8;
    let stack_size = (regexec_result_off + 40 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_match ---");
    emitter.label_global("__rt_preg_match");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_match stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // set new frame pointer

    // -- save inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // save pattern ptr
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // save pattern len
    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off));        // save subject ptr
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off));        // save subject len

    // -- strip delimiters from pattern --
    emitter.instruction("bl __rt_preg_strip");                                  // → x1=stripped, x2=len, x3=flags
    emitter.instruction(&format!("str x3, [sp, #{}]", flags_off));              // save flags

    // -- convert pattern from PCRE to POSIX and null-terminate --
    emitter.instruction("bl __rt_pcre_to_posix");                               // → x0=C string with PCRE shorthands converted
    emitter.instruction(&format!("str x0, [sp, #{}]", pattern_cstr_off));       // save pattern C string

    // -- compile regex: regcomp(&regex_t, pattern, flags) --
    emitter.instruction("mov x0, sp");                                          // x0 = regex_t at sp+0
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_cstr_off));       // x1 = pattern C string
    emitter.instruction("mov x2, #1");                                          // REG_EXTENDED
    emitter.instruction(&format!("ldr x9, [sp, #{}]", flags_off));              // load flags
    emitter.instruction("tst x9, #1");                                          // test icase
    emitter.instruction("b.eq __rt_preg_match_nocase");                         // skip if no icase
    emitter.instruction("orr x2, x2, #2");                                      // add REG_ICASE
    emitter.label("__rt_preg_match_nocase");
    emitter.bl_c("regcomp");                                                    // compile regex
    emitter.instruction("cbnz x0, __rt_preg_match_no");                         // compile failed → no match

    // -- null-terminate subject --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // load subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // load subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string

    // -- execute regex: regexec(&regex_t, subject, nmatch, &regmatch_t, eflags) --
    emitter.instruction("mov x0, sp");                                          // x0 = regex_t
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_cstr_off));       // x1 = subject C string
    emitter.instruction("mov x2, #1");                                          // nmatch = 1
    emitter.instruction(&format!("add x3, sp, #{}", regmatch_off));             // x3 = regmatch_t buffer
    emitter.instruction("mov x4, #0");                                          // eflags = 0
    emitter.bl_c("regexec");                                                    // regexec → x0=0 if match
    emitter.instruction(&format!("str x0, [sp, #{}]", regexec_result_off));     // save regexec result

    // -- free compiled regex --
    emitter.instruction("mov x0, sp");                                          // x0 = regex_t
    emitter.bl_c("regfree");                                                    // free compiled regex

    // -- return result --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", regexec_result_off));     // reload regexec result
    emitter.instruction("cbnz x0, __rt_preg_match_no");                         // non-zero = no match
    emitter.instruction("mov x0, #1");                                          // matched → return 1
    emitter.instruction("b __rt_preg_match_ret");                               // return

    emitter.label("__rt_preg_match_no");
    emitter.instruction("mov x0, #0");                                          // no match → return 0

    emitter.label("__rt_preg_match_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_preg_match_linux_x86_64(emitter: &mut Emitter) {
    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let subject_ptr_off = regmatch_off + emitter.platform.regmatch_t_size();
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let regexec_result_off = subject_cstr_off + 8;
    let stack_size = (regexec_result_off + 16 + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: preg_match ---");
    emitter.label_global("__rt_preg_match");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving regex compilation scratch storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the regex object, regmatch buffer, and subject spill slots
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve aligned local storage for regex_t, regmatch_t, and the helper spill fields
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", subject_ptr_off)); // preserve the elephc subject pointer across delimiter stripping and regex compilation helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", subject_len_off)); // preserve the elephc subject length across delimiter stripping and regex compilation helper calls
    emitter.instruction("mov rax, rdi");                                        // move the elephc pattern pointer into the preg-strip helper input register
    emitter.instruction("mov rdx, rsi");                                        // move the elephc pattern length into the preg-strip helper input register
    emitter.instruction("call __rt_preg_strip");                                // strip slash delimiters and collect supported regex flags from the elephc pattern payload
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", flags_off));  // preserve the regex flag word returned by the delimiter-strip helper for the later regcomp() call
    emitter.instruction("call __rt_pcre_to_posix");                             // translate PCRE shorthands into a POSIX-compatible null-terminated pattern string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off)); // preserve the converted C pattern pointer for the upcoming regcomp() call
    emitter.instruction("lea rdi, [rsp]");                                      // pass the local regex_t storage as the first regcomp() argument
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off)); // pass the converted POSIX pattern C string as the second regcomp() argument
    emitter.instruction("mov edx, 1");                                          // start from REG_EXTENDED when compiling the POSIX regex pattern
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", flags_off));  // reload the delimiter-strip helper flags before deciding whether to add REG_ICASE
    emitter.instruction("test rcx, 1");                                         // detect the supported case-insensitive regex flag gathered from the pattern literal
    emitter.instruction("jz __rt_preg_match_nocase_linux_x86_64");              // keep the default REG_EXTENDED flags when the pattern does not request case-insensitive matching
    emitter.instruction("or edx, 2");                                           // add REG_ICASE so regcomp() performs case-insensitive POSIX matching
    emitter.label("__rt_preg_match_nocase_linux_x86_64");
    emitter.bl_c("regcomp");                                                    // compile the translated POSIX pattern into the local regex_t storage
    emitter.instruction("test eax, eax");                                       // did regcomp() succeed and produce a compiled regex object?
    emitter.instruction("jnz __rt_preg_match_no_linux_x86_64");                 // failed regex compilation maps to a PHP false-style no-match result
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload the elephc subject pointer before null-terminating it in the secondary scratch buffer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload the elephc subject length before null-terminating it in the secondary scratch buffer
    emitter.instruction("call __rt_cstr2");                                     // materialize a null-terminated C version of the subject string for POSIX regexec()
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // preserve the subject C string pointer for the regexec() call and later cleanup path
    emitter.instruction("lea rdi, [rsp]");                                      // pass the compiled regex_t storage as the first regexec() argument
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", subject_cstr_off)); // pass the null-terminated subject C string as the second regexec() argument
    emitter.instruction("mov edx, 1");                                          // request exactly one regmatch_t capture because preg_match() only needs match/no-match
    emitter.instruction(&format!("lea rcx, [rsp + {}]", regmatch_off));         // pass the local regmatch_t buffer as the match output slot for regexec()
    emitter.instruction("xor r8d, r8d");                                        // pass eflags = 0 so POSIX regexec() performs a normal match from the subject start
    emitter.bl_c("regexec");                                                    // execute the compiled POSIX regex against the null-terminated subject string
    emitter.instruction(&format!("mov DWORD PTR [rsp + {}], eax", regexec_result_off)); // preserve the regexec() result code across the mandatory regfree() cleanup call
    emitter.instruction("lea rdi, [rsp]");                                      // reload the compiled regex_t storage before freeing it with regfree()
    emitter.bl_c("regfree");                                                    // release any internal POSIX regex resources held by the local regex_t object
    emitter.instruction(&format!("mov eax, DWORD PTR [rsp + {}]", regexec_result_off)); // reload the saved regexec() status code after regfree() clobbered caller-saved registers
    emitter.instruction("test eax, eax");                                       // interpret a zero regexec() result as a successful regex match
    emitter.instruction("jnz __rt_preg_match_no_linux_x86_64");                 // return zero when POSIX regexec() reports no match
    emitter.instruction("mov eax, 1");                                          // return one when POSIX regexec() reports a successful match
    emitter.instruction("jmp __rt_preg_match_ret_linux_x86_64");                // share the common epilogue after materializing the successful match result

    emitter.label("__rt_preg_match_no_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // return zero for compile failures and subjects that do not match the regex

    emitter.label("__rt_preg_match_ret_linux_x86_64");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release the local regex_t, regmatch_t, and subject spill storage before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the regex helper completes
    emitter.instruction("ret");                                                 // return the preg_match() integer result in the x86_64 integer result register
}
