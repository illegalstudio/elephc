use crate::codegen::{emit::Emitter, platform::Arch};

/// __rt_preg_match_all: count all non-overlapping matches of regex in subject.
/// Input:  x1=pattern ptr, x2=pattern len, x3=subject ptr, x4=subject len
/// Output: x0=match count
pub(crate) fn emit_preg_match_all(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_match_all_linux_x86_64(emitter);
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
    let match_count_off = subject_cstr_off + 8;
    let current_pos_off = match_count_off + 8;
    let stack_size = (current_pos_off + 48 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_match_all ---");
    emitter.label_global("__rt_preg_match_all");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_match_all stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // set new frame pointer

    // -- save inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // save pattern ptr
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // save pattern len
    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off));        // save subject ptr
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off));        // save subject len

    // -- strip delimiters --
    emitter.instruction("bl __rt_preg_strip");                                  // → x1, x2, x3=flags
    emitter.instruction(&format!("str x3, [sp, #{}]", flags_off));              // save flags

    // -- convert pattern from PCRE to POSIX and null-terminate --
    emitter.instruction("bl __rt_pcre_to_posix");                               // → x0=C string with PCRE shorthands converted
    emitter.instruction(&format!("str x0, [sp, #{}]", pattern_cstr_off));       // save pattern C string

    // -- compile regex --
    emitter.instruction("mov x0, sp");                                          // regex_t at sp
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_cstr_off));       // pattern
    emitter.instruction("mov x2, #1");                                          // REG_EXTENDED
    emitter.instruction(&format!("ldr x9, [sp, #{}]", flags_off));              // flags
    emitter.instruction("tst x9, #1");                                          // test icase
    emitter.instruction("b.eq __rt_preg_match_all_nc");                         // skip
    emitter.instruction("orr x2, x2, #2");                                      // REG_ICASE
    emitter.label("__rt_preg_match_all_nc");
    emitter.bl_c("regcomp");                                                    // compile
    emitter.instruction("cbnz x0, __rt_preg_match_all_fail");                   // fail

    // -- null-terminate subject --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string

    // -- count matches loop --
    emitter.instruction(&format!("str xzr, [sp, #{}]", match_count_off));       // match count = 0
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_cstr_off));       // current position = start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_pos_off));        // save current pos

    emitter.label("__rt_preg_match_all_loop");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_pos_off));        // current subject position
    emitter.instruction("ldrb w9, [x1]");                                       // load byte at current pos
    emitter.instruction("cbz w9, __rt_preg_match_all_done");                    // null terminator = done
    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.instruction("mov x2, #1");                                          // nmatch = 1
    emitter.instruction(&format!("add x3, sp, #{}", regmatch_off));             // regmatch_t buffer
    emitter.instruction("mov x4, #0");                                          // eflags = 0
    emitter.bl_c("regexec");                                                    // execute
    emitter.instruction("cbnz x0, __rt_preg_match_all_done");                   // no more matches

    // -- found a match, increment count --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", match_count_off));        // load count
    emitter.instruction("add x9, x9, #1");                                      // increment
    emitter.instruction(&format!("str x9, [sp, #{}]", match_count_off));        // save count

    // -- advance past this match (rm_eo is 8 bytes at sp+40) --
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // current pos
    emitter.instruction(&emitter.platform.regoff_load_instr("x11", "sp", regmatch_off + emitter.platform.regmatch_rm_eo_offset())); // load rm_eo with the native regoff_t width
    emitter.instruction("cmp x11, #0");                                         // check for zero-length match
    emitter.instruction("b.gt __rt_preg_match_all_adv");                        // non-zero advance
    emitter.instruction("mov x11, #1");                                         // advance by at least 1
    emitter.label("__rt_preg_match_all_adv");
    emitter.instruction("add x10, x10, x11");                                   // advance position
    emitter.instruction(&format!("str x10, [sp, #{}]", current_pos_off));       // save new position
    emitter.instruction("b __rt_preg_match_all_loop");                          // continue

    emitter.label("__rt_preg_match_all_done");
    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.bl_c("regfree");                                                    // free
    emitter.instruction(&format!("ldr x0, [sp, #{}]", match_count_off));        // return count
    emitter.instruction("b __rt_preg_match_all_ret");                           // return

    emitter.label("__rt_preg_match_all_fail");
    emitter.instruction("mov x0, #0");                                          // return 0 on compile failure

    emitter.label("__rt_preg_match_all_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_preg_match_all_linux_x86_64(emitter: &mut Emitter) {
    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let subject_ptr_off = regmatch_off + emitter.platform.regmatch_t_size();
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let match_count_off = subject_cstr_off + 8;
    let current_pos_off = match_count_off + 8;
    let stack_size = (current_pos_off + 16 + 15) & !15;
    let rm_eo_off = regmatch_off + emitter.platform.regmatch_rm_eo_offset();
    let load_rm_eo = if emitter.platform.regmatch_t_size() == 16 {
        format!("mov r11, QWORD PTR [rsp + {}]", rm_eo_off)
    } else {
        format!("movsxd r11, DWORD PTR [rsp + {}]", rm_eo_off)
    };

    emitter.blank();
    emitter.comment("--- runtime: preg_match_all ---");
    emitter.label_global("__rt_preg_match_all");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving regex-counting scratch storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the regex object, regmatch buffer, and loop spill slots
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve aligned local storage for regex_t, regmatch_t, and match-count bookkeeping
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", subject_ptr_off)); // preserve the elephc subject pointer across delimiter stripping and regex compilation helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", subject_len_off)); // preserve the elephc subject length across delimiter stripping and regex compilation helper calls
    emitter.instruction("mov rax, rdi");                                        // move the elephc pattern pointer into the delimiter-strip helper input register
    emitter.instruction("mov rdx, rsi");                                        // move the elephc pattern length into the delimiter-strip helper input register
    emitter.instruction("call __rt_preg_strip");                                // strip slash delimiters and gather supported regex flags from the pattern literal
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", flags_off));  // preserve the delimiter-strip helper flags for the later regcomp() call
    emitter.instruction("call __rt_pcre_to_posix");                             // translate PCRE shorthands into a POSIX-compatible null-terminated pattern string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off)); // preserve the converted POSIX pattern C string across compilation and loop setup
    emitter.instruction("lea rdi, [rsp]");                                      // pass the local regex_t storage as the first regcomp() argument
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off)); // pass the converted POSIX pattern C string as the second regcomp() argument
    emitter.instruction("mov edx, 1");                                          // start from REG_EXTENDED when compiling the POSIX regex pattern
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", flags_off));  // reload the delimiter-strip helper flags before deciding whether to add REG_ICASE
    emitter.instruction("test rcx, 1");                                         // detect the supported case-insensitive regex modifier from the pattern literal
    emitter.instruction("jz __rt_preg_match_all_nocase_linux_x86_64");          // keep the default REG_EXTENDED flags when the pattern does not request case-insensitive matching
    emitter.instruction("or edx, 2");                                           // add REG_ICASE so regcomp() performs case-insensitive POSIX matching
    emitter.label("__rt_preg_match_all_nocase_linux_x86_64");
    emitter.bl_c("regcomp");                                                    // compile the translated POSIX pattern into the local regex_t storage
    emitter.instruction("test eax, eax");                                       // did regcomp() succeed and produce a compiled regex object?
    emitter.instruction("jnz __rt_preg_match_all_fail_linux_x86_64");           // failed regex compilation maps to a zero-count result
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload the elephc subject pointer before null-terminating it in the secondary scratch buffer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload the elephc subject length before null-terminating it in the secondary scratch buffer
    emitter.instruction("call __rt_cstr2");                                     // materialize a null-terminated subject C string for repeated POSIX regexec() probes
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // preserve the subject C string pointer across the full match-counting loop
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", match_count_off)); // initialize the running non-overlapping match count at zero
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", current_pos_off)); // start the current subject cursor at the beginning of the null-terminated subject C string

    emitter.label("__rt_preg_match_all_loop_linux_x86_64");
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_pos_off)); // reload the current subject C-string cursor before the next regexec() probe
    emitter.instruction("movzx r9d, BYTE PTR [rsi]");                           // check whether the current subject cursor already points at the trailing null terminator
    emitter.instruction("test r9d, r9d");                                       // treat the terminating null byte as the loop completion condition
    emitter.instruction("jz __rt_preg_match_all_done_linux_x86_64");            // stop counting once the full null-terminated subject has been consumed
    emitter.instruction("lea rdi, [rsp]");                                      // pass the compiled regex_t storage as the first regexec() argument
    emitter.instruction("mov edx, 1");                                          // request exactly one regmatch_t capture because the loop only needs the overall match extent
    emitter.instruction(&format!("lea rcx, [rsp + {}]", regmatch_off));         // pass the local regmatch_t buffer as the match extent output slot
    emitter.instruction("xor r8d, r8d");                                        // pass eflags = 0 so regexec() matches from the current subject cursor
    emitter.bl_c("regexec");                                                    // execute the compiled POSIX regex at the current subject cursor
    emitter.instruction("test eax, eax");                                       // did regexec() find another match at or after the current cursor?
    emitter.instruction("jnz __rt_preg_match_all_done_linux_x86_64");           // stop counting when regexec() reports no further matches
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", match_count_off)); // reload the running non-overlapping match count before incrementing it
    emitter.instruction("add r9, 1");                                           // count the newly discovered regex match
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", match_count_off)); // preserve the updated match count for the next loop iteration
    emitter.instruction(&load_rm_eo);                                           // load rm_eo from the native Linux regmatch_t layout using the correct regoff_t width
    emitter.instruction("cmp r11, 0");                                          // detect zero-length regex matches so the loop still makes forward progress
    emitter.instruction("jg __rt_preg_match_all_adv_linux_x86_64");             // use the reported end offset directly when the regex consumed at least one byte
    emitter.instruction("mov r11, 1");                                          // force zero-length matches to advance by one byte and avoid infinite loops
    emitter.label("__rt_preg_match_all_adv_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_pos_off)); // reload the current subject cursor before advancing it past the latest regex match
    emitter.instruction("add r10, r11");                                        // advance the current subject cursor by rm_eo bytes or the forced one-byte fallback
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", current_pos_off)); // preserve the advanced subject cursor for the next loop iteration
    emitter.instruction("jmp __rt_preg_match_all_loop_linux_x86_64");           // continue counting the remaining non-overlapping regex matches

    emitter.label("__rt_preg_match_all_done_linux_x86_64");
    emitter.instruction("lea rdi, [rsp]");                                      // reload the compiled regex_t storage before freeing it with regfree()
    emitter.bl_c("regfree");                                                    // release the compiled POSIX regex resources before returning the match count
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", match_count_off)); // return the total number of non-overlapping regex matches discovered in the subject
    emitter.instruction("jmp __rt_preg_match_all_ret_linux_x86_64");            // share the common epilogue after the successful match-count path

    emitter.label("__rt_preg_match_all_fail_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // return zero when regex compilation fails for preg_match_all()

    emitter.label("__rt_preg_match_all_ret_linux_x86_64");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release the local regex_t, regmatch_t, and counting spill storage before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the regex-count helper completes
    emitter.instruction("ret");                                                 // return the preg_match_all() integer count in the x86_64 integer result register
}
