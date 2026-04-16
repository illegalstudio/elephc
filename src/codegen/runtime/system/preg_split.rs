use crate::codegen::{emit::Emitter, platform::Arch};

/// __rt_preg_split: split a string by regex pattern.
/// Input:  x1=pattern ptr, x2=pattern len, x3=subject ptr, x4=subject len
/// Output: x0=array pointer (string array)
pub(crate) fn emit_preg_split(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_split_linux_x86_64(emitter);
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
    let array_ptr_off = pattern_cstr_off + 8;
    let subject_cstr_off = array_ptr_off + 8;
    let current_cstr_off = subject_cstr_off + 8;
    let current_elephc_off = current_cstr_off + 8;
    let stack_size = (current_elephc_off + 96 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_split ---");
    emitter.label_global("__rt_preg_split");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_split stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // set new frame pointer

    // -- save inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // pattern ptr
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // pattern len
    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off));        // subject ptr (elephc)
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off));        // subject len

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
    emitter.instruction("b.eq __rt_preg_split_nc");                             // skip
    emitter.instruction("orr x2, x2, #2");                                      // REG_ICASE
    emitter.label("__rt_preg_split_nc");
    emitter.bl_c("regcomp");                                                    // compile
    emitter.instruction("cbnz x0, __rt_preg_split_fail");                       // fail

    // -- create new string array --
    emitter.instruction("mov x0, #8");                                          // initial capacity
    emitter.instruction("mov x1, #16");                                         // element size = 16 (ptr + len for strings)
    emitter.instruction("bl __rt_array_new");                                   // create array → x0
    emitter.instruction(&format!("str x0, [sp, #{}]", array_ptr_off));          // save array ptr

    // -- null-terminate subject --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string

    // -- initialize positions --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_cstr_off));       // C string start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_cstr_off));       // current C string pos
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_ptr_off));        // elephc ptr start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_elephc_off));     // current elephc ptr

    // -- split loop --
    emitter.label("__rt_preg_split_loop");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_cstr_off));       // current C string pos
    emitter.instruction("ldrb w9, [x1]");                                       // check for end
    emitter.instruction("cbz w9, __rt_preg_split_last");                        // end of string, add final segment

    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.instruction("mov x2, #1");                                          // nmatch
    emitter.instruction(&format!("add x3, sp, #{}", regmatch_off));             // regmatch_t buffer
    emitter.instruction("mov x4, #0");                                          // eflags
    emitter.bl_c("regexec");                                                    // execute
    emitter.instruction("cbnz x0, __rt_preg_split_last");                       // no more matches

    // -- add segment before match to array --
    emitter.instruction(&emitter.platform.regoff_load_instr("x9", "sp", regmatch_off)); // load rm_so with the native regoff_t width
    emitter.instruction(&format!("ldr x0, [sp, #{}]", array_ptr_off));          // array ptr
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_elephc_off));     // current elephc ptr
    emitter.instruction("mov x2, x9");                                          // segment length = rm_so
    emitter.instruction("bl __rt_array_push_str");                              // push string to array
    emitter.instruction(&format!("str x0, [sp, #{}]", array_ptr_off));          // save (possibly reallocated) array ptr

    // -- advance past match --
    emitter.instruction(&emitter.platform.regoff_load_instr("x9", "sp", regmatch_off + emitter.platform.regmatch_rm_eo_offset())); // load rm_eo with the native regoff_t width
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_cstr_off));      // current C string pos
    emitter.instruction("add x10, x10, x9");                                    // advance C string pos
    emitter.instruction(&format!("str x10, [sp, #{}]", current_cstr_off));      // save
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_elephc_off));    // current elephc ptr
    emitter.instruction("add x10, x10, x9");                                    // advance elephc ptr
    emitter.instruction(&format!("str x10, [sp, #{}]", current_elephc_off));    // save
    emitter.instruction("b __rt_preg_split_loop");                              // continue

    // -- add last segment --
    emitter.label("__rt_preg_split_last");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_elephc_off));    // current elephc ptr
    emitter.instruction(&format!("ldr x11, [sp, #{}]", subject_ptr_off));       // original subject ptr
    emitter.instruction(&format!("ldr x12, [sp, #{}]", subject_len_off));       // original subject len
    emitter.instruction("add x11, x11, x12");                                   // end of subject
    emitter.instruction("sub x2, x11, x10");                                    // remaining length

    emitter.instruction(&format!("ldr x0, [sp, #{}]", array_ptr_off));          // array ptr
    emitter.instruction("mov x1, x10");                                         // segment ptr
    emitter.instruction("bl __rt_array_push_str");                              // push last segment
    emitter.instruction(&format!("str x0, [sp, #{}]", array_ptr_off));          // save array ptr

    // -- free regex and return --
    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.bl_c("regfree");                                                    // free
    emitter.instruction(&format!("ldr x0, [sp, #{}]", array_ptr_off));          // return array ptr
    emitter.instruction("b __rt_preg_split_ret");                               // return

    // -- failure: return empty array --
    emitter.label("__rt_preg_split_fail");
    emitter.instruction("mov x0, #4");                                          // small capacity
    emitter.instruction("mov x1, #16");                                         // element size = 16 for string array
    emitter.instruction("bl __rt_array_new");                                   // create empty array

    emitter.label("__rt_preg_split_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_preg_split_linux_x86_64(emitter: &mut Emitter) {
    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let subject_ptr_off = regmatch_off + emitter.platform.regmatch_t_size();
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let array_ptr_off = pattern_cstr_off + 8;
    let subject_cstr_off = array_ptr_off + 8;
    let current_cstr_off = subject_cstr_off + 8;
    let current_elephc_off = current_cstr_off + 8;
    let stack_size = (current_elephc_off + 16 + 15) & !15;
    let load_rm_so = if emitter.platform.regmatch_t_size() == 16 {
        format!("mov r9, QWORD PTR [rsp + {}]", regmatch_off)
    } else {
        format!("movsxd r9, DWORD PTR [rsp + {}]", regmatch_off)
    };
    let load_rm_eo = if emitter.platform.regmatch_t_size() == 16 {
        format!(
            "mov r9, QWORD PTR [rsp + {}]",
            regmatch_off + emitter.platform.regmatch_rm_eo_offset()
        )
    } else {
        format!(
            "movsxd r9, DWORD PTR [rsp + {}]",
            regmatch_off + emitter.platform.regmatch_rm_eo_offset()
        )
    };

    emitter.blank();
    emitter.comment("--- runtime: preg_split ---");
    emitter.label_global("__rt_preg_split");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving regex-split scratch storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the regex object, regmatch buffer, and split spill slots
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve aligned local storage for regex_t, regmatch_t, and split bookkeeping
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", subject_ptr_off)); // preserve the elephc subject pointer across delimiter stripping and regex compilation helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", subject_len_off)); // preserve the elephc subject length across delimiter stripping and regex compilation helper calls
    emitter.instruction("mov rax, rdi");                                        // move the elephc pattern pointer into the delimiter-strip helper input register
    emitter.instruction("mov rdx, rsi");                                        // move the elephc pattern length into the delimiter-strip helper input register
    emitter.instruction("call __rt_preg_strip");                                // strip slash delimiters and gather supported regex flags from the pattern literal
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", flags_off));  // preserve the delimiter-strip helper flags for the later regcomp() call
    emitter.instruction("call __rt_pcre_to_posix");                             // translate PCRE shorthands into a POSIX-compatible null-terminated pattern string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off)); // preserve the converted POSIX pattern C string across compilation and splitting
    emitter.instruction("lea rdi, [rsp]");                                      // pass the local regex_t storage as the first regcomp() argument
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off)); // pass the converted POSIX pattern C string as the second regcomp() argument
    emitter.instruction("mov edx, 1");                                          // start from REG_EXTENDED when compiling the POSIX regex pattern
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", flags_off));  // reload the delimiter-strip helper flags before deciding whether to add REG_ICASE
    emitter.instruction("test rcx, 1");                                         // detect the supported case-insensitive regex modifier from the pattern literal
    emitter.instruction("jz __rt_preg_split_nocase_linux_x86_64");              // keep the default REG_EXTENDED flags when the pattern does not request case-insensitive matching
    emitter.instruction("or edx, 2");                                           // add REG_ICASE so regcomp() performs case-insensitive POSIX matching
    emitter.label("__rt_preg_split_nocase_linux_x86_64");
    emitter.bl_c("regcomp");                                                    // compile the translated POSIX pattern into the local regex_t storage
    emitter.instruction("test eax, eax");                                       // did regcomp() succeed and produce a compiled regex object?
    emitter.instruction("jnz __rt_preg_split_fail_linux_x86_64");               // failed regex compilation maps to returning an empty split-result array
    emitter.instruction("mov edi, 8");                                          // request a small initial capacity for the string split-result array
    emitter.instruction("mov esi, 16");                                         // request 16-byte element slots because the split-result array stores ptr/len string pairs
    emitter.instruction("call __rt_array_new");                                 // allocate the initial string split-result array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", array_ptr_off)); // preserve the current split-result array pointer across helper calls and optional growth
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload the elephc subject pointer before null-terminating it in the secondary scratch buffer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload the elephc subject length before null-terminating it in the secondary scratch buffer
    emitter.instruction("call __rt_cstr2");                                     // materialize a null-terminated subject C string for repeated POSIX regexec() probes
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // preserve the subject C string pointer across the full split loop
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", current_cstr_off)); // initialize the current subject C-string cursor from the start of the null-terminated subject string
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload the original elephc subject pointer as the initial split-segment payload cursor
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", current_elephc_off)); // initialize the current elephc payload cursor for split-result segment extraction

    emitter.label("__rt_preg_split_loop_linux_x86_64");
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_cstr_off)); // reload the current subject C-string cursor before the next regexec() probe
    emitter.instruction("movzx r9d, BYTE PTR [rsi]");                           // stop the split loop once the current subject cursor reaches the trailing null terminator
    emitter.instruction("test r9d, r9d");                                       // treat the terminating null byte as the end-of-subject condition
    emitter.instruction("jz __rt_preg_split_last_linux_x86_64");                // emit the final trailing segment once the full subject payload has been consumed
    emitter.instruction("lea rdi, [rsp]");                                      // pass the compiled regex_t storage as the first regexec() argument
    emitter.instruction("mov edx, 1");                                          // request exactly one regmatch_t capture because splitting only needs the full match extent
    emitter.instruction(&format!("lea rcx, [rsp + {}]", regmatch_off));         // pass the local regmatch_t buffer as the match extent output slot
    emitter.instruction("xor r8d, r8d");                                        // pass eflags = 0 so regexec() matches from the current subject cursor
    emitter.bl_c("regexec");                                                    // execute the compiled POSIX regex at the current subject cursor
    emitter.instruction("test eax, eax");                                       // did regexec() find another separator match at or after the current cursor?
    emitter.instruction("jnz __rt_preg_split_last_linux_x86_64");               // emit the final trailing segment once regexec() reports no further separators
    emitter.instruction(&load_rm_so);                                           // load rm_so from the native Linux regmatch_t layout using the correct regoff_t width
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", array_ptr_off)); // reload the current split-result array pointer before pushing the segment preceding the separator match
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_elephc_off)); // reload the current elephc payload cursor as the next split-result segment start pointer
    emitter.instruction("mov rdx, r9");                                         // pass the pre-match segment length derived from rm_so to the string-array push helper
    emitter.instruction("call __rt_array_push_str");                            // append the pre-separator string segment into the split-result array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", array_ptr_off)); // preserve the possibly reallocated split-result array pointer after appending the new segment
    emitter.instruction(&load_rm_eo);                                           // load rm_eo from the native Linux regmatch_t layout using the correct regoff_t width
    emitter.instruction("cmp r9, 0");                                           // detect zero-length regex matches so the split loop still makes forward progress
    emitter.instruction("jg __rt_preg_split_advance_ok_linux_x86_64");          // trust rm_eo directly when the separator regex consumed at least one byte
    emitter.instruction("mov r9, 1");                                           // force zero-length matches to advance by one byte and avoid infinite split loops
    emitter.label("__rt_preg_split_advance_ok_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_cstr_off)); // reload the current subject C-string cursor before advancing it past the separator match
    emitter.instruction("add r10, r9");                                         // advance the current subject C-string cursor by rm_eo bytes or the forced one-byte fallback
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", current_cstr_off)); // preserve the advanced subject C-string cursor for the next split-loop iteration
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_elephc_off)); // reload the current elephc payload cursor before advancing it past the separator match
    emitter.instruction("add r10, r9");                                         // advance the current elephc payload cursor by the same byte distance as the C-string cursor
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", current_elephc_off)); // preserve the advanced elephc payload cursor for the next split-result segment
    emitter.instruction("jmp __rt_preg_split_loop_linux_x86_64");               // continue splitting the remaining subject payload after the separator match

    emitter.label("__rt_preg_split_last_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_elephc_off)); // reload the current elephc payload cursor as the start of the final trailing split-result segment
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload the original elephc subject pointer before computing the end of the full subject payload
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload the original elephc subject length before computing the trailing-segment length
    emitter.instruction("add r11, rdx");                                        // compute the end address of the original elephc subject payload
    emitter.instruction("sub r11, r10");                                        // compute the trailing-segment length from the subject end pointer and current payload cursor
    emitter.instruction("mov rdx, r11");                                        // pass the trailing-segment length in the register expected by the string-array push helper
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", array_ptr_off)); // reload the current split-result array pointer before pushing the trailing segment
    emitter.instruction("mov rsi, r10");                                        // pass the trailing segment start pointer to the string-array push helper
    emitter.instruction("call __rt_array_push_str");                            // append the trailing split-result segment into the split-result array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", array_ptr_off)); // preserve the possibly reallocated split-result array pointer after appending the trailing segment
    emitter.instruction("lea rdi, [rsp]");                                      // reload the compiled regex_t storage before freeing it with regfree()
    emitter.bl_c("regfree");                                                    // release the compiled POSIX regex resources before returning the split-result array
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", array_ptr_off)); // return the final split-result array pointer in the x86_64 integer result register
    emitter.instruction("jmp __rt_preg_split_ret_linux_x86_64");                // share the common epilogue after materializing the split-result array pointer

    emitter.label("__rt_preg_split_fail_linux_x86_64");
    emitter.instruction("mov edi, 4");                                          // request a tiny capacity for the fallback empty split-result array
    emitter.instruction("mov esi, 16");                                         // request 16-byte element slots because the split-result array stores ptr/len string pairs
    emitter.instruction("call __rt_array_new");                                 // allocate the fallback empty split-result array when regex compilation fails

    emitter.label("__rt_preg_split_ret_linux_x86_64");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release the local regex_t, regmatch_t, and split spill storage before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the regex-split helper completes
    emitter.instruction("ret");                                                 // return the preg_split() array result in the x86_64 integer result register
}
