use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// __rt_preg_replace: replace all regex matches in subject string.
/// Input:  x1=pattern ptr, x2=pattern len, x3=replacement ptr, x4=replacement len,
///         x5=subject ptr, x6=subject len
/// Output: x1=result ptr, x2=result len
pub(crate) fn emit_preg_replace(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_replace_linux_x86_64(emitter);
        return;
    }

    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let pattern_ptr_off = regmatch_off + emitter.platform.regmatch_t_size();
    let pattern_len_off = pattern_ptr_off + 8;
    let replacement_ptr_off = pattern_len_off + 8;
    let replacement_len_off = replacement_ptr_off + 8;
    let subject_ptr_off = replacement_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let output_start_off = subject_cstr_off + 8;
    let output_write_off = output_start_off + 8;
    let current_pos_off = output_write_off + 8;
    let stack_size = (current_pos_off + 96 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_replace ---");
    emitter.label_global("__rt_preg_replace");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_replace stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // set new frame pointer

    // -- save all inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // pattern ptr
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // pattern len
    emitter.instruction(&format!("str x3, [sp, #{}]", replacement_ptr_off));    // replacement ptr
    emitter.instruction(&format!("str x4, [sp, #{}]", replacement_len_off));    // replacement len
    emitter.instruction(&format!("str x5, [sp, #{}]", subject_ptr_off));        // subject ptr
    emitter.instruction(&format!("str x6, [sp, #{}]", subject_len_off));        // subject len

    // -- strip delimiters from pattern --
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
    emitter.instruction("b.eq __rt_preg_replace_nc");                           // skip
    emitter.instruction("orr x2, x2, #2");                                      // REG_ICASE
    emitter.label("__rt_preg_replace_nc");
    emitter.bl_c("regcomp");                                                    // compile
    emitter.instruction("cbnz x0, __rt_preg_replace_fail");                     // fail → return original

    // -- null-terminate subject --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string

    // -- set up output buffer in concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // output position
    emitter.instruction(&format!("str x11, [sp, #{}]", output_start_off));      // save output start
    emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off));      // save output write pos

    // -- initialize current position --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_cstr_off));       // subject C string start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_pos_off));        // save current pos

    // -- replacement loop --
    emitter.label("__rt_preg_replace_loop");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_pos_off));        // current pos
    emitter.instruction("ldrb w9, [x1]");                                       // check for end
    emitter.instruction("cbz w9, __rt_preg_replace_done");                      // end of string

    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.instruction("mov x2, #1");                                          // nmatch
    emitter.instruction(&format!("add x3, sp, #{}", regmatch_off));             // regmatch_t buffer
    emitter.instruction("mov x4, #0");                                          // eflags
    emitter.bl_c("regexec");                                                    // execute
    emitter.instruction("cbnz x0, __rt_preg_replace_tail");                     // no more matches, copy rest

    // -- copy text before match (rm_so bytes) --
    emitter.instruction(&emitter.platform.regoff_load_instr("x9", "sp", regmatch_off)); // load rm_so with the native regoff_t width
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // current C string pos
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // output write pos
    emitter.instruction("mov x12, #0");                                         // copy index
    emitter.label("__rt_preg_replace_pre");
    emitter.instruction("cmp x12, x9");                                         // check if done
    emitter.instruction("b.ge __rt_preg_replace_repl");                         // done copying prefix
    emitter.instruction("ldrb w13, [x10, x12]");                                // load byte
    emitter.instruction("strb w13, [x11]");                                     // write byte
    emitter.instruction("add x11, x11, #1");                                    // advance output
    emitter.instruction("add x12, x12, #1");                                    // increment
    emitter.instruction("b __rt_preg_replace_pre");                             // continue

    // -- copy replacement string --
    emitter.label("__rt_preg_replace_repl");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", replacement_ptr_off));    // replacement ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", replacement_len_off));    // replacement len
    emitter.instruction("mov x12, #0");                                         // copy index
    emitter.label("__rt_preg_replace_repl_copy");
    emitter.instruction("cmp x12, x2");                                         // check if done
    emitter.instruction("b.ge __rt_preg_replace_advance");                      // done
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load replacement byte
    emitter.instruction("strb w13, [x11]");                                     // write byte
    emitter.instruction("add x11, x11, #1");                                    // advance output
    emitter.instruction("add x12, x12, #1");                                    // increment
    emitter.instruction("b __rt_preg_replace_repl_copy");                       // continue

    // -- advance past match --
    emitter.label("__rt_preg_replace_advance");
    emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off));      // save output write pos
    emitter.instruction(&emitter.platform.regoff_load_instr("x9", "sp", regmatch_off + emitter.platform.regmatch_rm_eo_offset())); // load rm_eo with the native regoff_t width
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // current pos
    emitter.instruction("add x10, x10, x9");                                    // advance past match
    emitter.instruction(&format!("str x10, [sp, #{}]", current_pos_off));       // save new pos
    emitter.instruction("b __rt_preg_replace_loop");                            // continue

    // -- copy remaining text after last match --
    emitter.label("__rt_preg_replace_tail");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // current pos
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // output write pos
    emitter.label("__rt_preg_replace_tail_loop");
    emitter.instruction("ldrb w9, [x10]");                                      // load byte
    emitter.instruction("cbz w9, __rt_preg_replace_done");                      // null terminator = done
    emitter.instruction("strb w9, [x11]");                                      // write byte
    emitter.instruction("add x10, x10, #1");                                    // advance source
    emitter.instruction("add x11, x11, #1");                                    // advance output
    emitter.instruction("b __rt_preg_replace_tail_loop");                       // continue

    emitter.label("__rt_preg_replace_done");
    emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off));      // save final write pos
    // -- free regex --
    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.bl_c("regfree");                                                    // free

    // -- compute result --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", output_start_off));       // output start
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // output end
    emitter.instruction("sub x2, x11, x1");                                     // result length

    // -- update concat_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x2");                                    // add result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset
    emitter.instruction("b __rt_preg_replace_ret");                             // return

    // -- failure: return original subject --
    emitter.label("__rt_preg_replace_fail");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // original subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // original subject len

    emitter.label("__rt_preg_replace_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_preg_replace_linux_x86_64(emitter: &mut Emitter) {
    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let pattern_ptr_off = regmatch_off + emitter.platform.regmatch_t_size();
    let pattern_len_off = pattern_ptr_off + 8;
    let replacement_ptr_off = pattern_len_off + 8;
    let replacement_len_off = replacement_ptr_off + 8;
    let subject_ptr_off = replacement_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let output_start_off = subject_cstr_off + 8;
    let output_write_off = output_start_off + 8;
    let current_pos_off = output_write_off + 8;
    let stack_size = (current_pos_off + 16 + 15) & !15;
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
    emitter.comment("--- runtime: preg_replace ---");
    emitter.label_global("__rt_preg_replace");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving regex-replace scratch storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the regex object, regmatch buffer, and replace spill slots
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve aligned local storage for regex_t, regmatch_t, and replacement bookkeeping
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdi", pattern_ptr_off)); // preserve the elephc pattern pointer across regex helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rsi", pattern_len_off)); // preserve the elephc pattern length across regex helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", replacement_ptr_off)); // preserve the elephc replacement pointer across regex helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", replacement_len_off)); // preserve the elephc replacement length across regex helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r8", subject_ptr_off)); // preserve the elephc subject pointer across regex helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", subject_len_off)); // preserve the elephc subject length across regex helper calls
    emitter.instruction("mov rax, rdi");                                        // move the elephc pattern pointer into the delimiter-strip helper input register
    emitter.instruction("mov rdx, rsi");                                        // move the elephc pattern length into the delimiter-strip helper input register
    emitter.instruction("call __rt_preg_strip");                                // strip slash delimiters and gather supported regex flags from the pattern literal
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", flags_off));  // preserve the delimiter-strip helper flags for the later regcomp() call
    emitter.instruction("call __rt_pcre_to_posix");                             // translate PCRE shorthands into a POSIX-compatible null-terminated pattern string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off)); // preserve the converted POSIX pattern C string across compilation and replacement
    emitter.instruction("lea rdi, [rsp]");                                      // pass the local regex_t storage as the first regcomp() argument
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off)); // pass the converted POSIX pattern C string as the second regcomp() argument
    emitter.instruction("mov edx, 1");                                          // start from REG_EXTENDED when compiling the POSIX regex pattern
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", flags_off));  // reload the delimiter-strip helper flags before deciding whether to add REG_ICASE
    emitter.instruction("test rcx, 1");                                         // detect the supported case-insensitive regex modifier from the pattern literal
    emitter.instruction("jz __rt_preg_replace_nocase_linux_x86_64");            // keep the default REG_EXTENDED flags when the pattern does not request case-insensitive matching
    emitter.instruction("or edx, 2");                                           // add REG_ICASE so regcomp() performs case-insensitive POSIX matching
    emitter.label("__rt_preg_replace_nocase_linux_x86_64");
    emitter.bl_c("regcomp");                                                    // compile the translated POSIX pattern into the local regex_t storage
    emitter.instruction("test eax, eax");                                       // did regcomp() succeed and produce a compiled regex object?
    emitter.instruction("jnz __rt_preg_replace_fail_linux_x86_64");             // failed regex compilation maps to returning the original subject unchanged
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload the elephc subject pointer before null-terminating it in the secondary scratch buffer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload the elephc subject length before null-terminating it in the secondary scratch buffer
    emitter.instruction("call __rt_cstr2");                                     // materialize a null-terminated subject C string for repeated POSIX regexec() probes
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // preserve the subject C string pointer across the full replacement loop
    abi::emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current concat scratch-buffer offset before appending the replacement output
    abi::emit_symbol_address(emitter, "rax", "_concat_buf");
    emitter.instruction("add rax, r11");                                        // compute the first free byte inside the concat scratch buffer for the replacement output
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", output_start_off)); // preserve the start of the replacement output string inside the concat scratch buffer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", output_write_off)); // initialize the replacement output write cursor from the output start pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", current_pos_off)); // reserve the current position slot before storing the initial subject cursor
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_cstr_off)); // reload the subject C string pointer as the initial regex execution cursor
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", current_pos_off)); // initialize the current subject cursor from the start of the null-terminated subject string

    emitter.label("__rt_preg_replace_loop_linux_x86_64");
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_pos_off)); // reload the current subject C-string cursor before the next regexec() probe
    emitter.instruction("movzx r9d, BYTE PTR [rsi]");                           // stop the replacement loop once the current subject cursor reaches the trailing null terminator
    emitter.instruction("test r9d, r9d");                                       // treat the terminating null byte as the end-of-subject condition
    emitter.instruction("jz __rt_preg_replace_done_linux_x86_64");              // finish replacement when the full subject payload has been consumed
    emitter.instruction("lea rdi, [rsp]");                                      // pass the compiled regex_t storage as the first regexec() argument
    emitter.instruction("mov edx, 1");                                          // request exactly one regmatch_t capture because replacement only needs the full match extent
    emitter.instruction(&format!("lea rcx, [rsp + {}]", regmatch_off));         // pass the local regmatch_t buffer as the match extent output slot
    emitter.instruction("xor r8d, r8d");                                        // pass eflags = 0 so regexec() matches from the current subject cursor
    emitter.bl_c("regexec");                                                    // execute the compiled POSIX regex at the current subject cursor
    emitter.instruction("test eax, eax");                                       // did regexec() find another match at or after the current cursor?
    emitter.instruction("jnz __rt_preg_replace_tail_linux_x86_64");             // copy the remaining subject tail once regexec() reports no further matches
    emitter.instruction(&load_rm_so);                                           // load rm_so from the native Linux regmatch_t layout using the correct regoff_t width
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_pos_off)); // reload the current subject cursor before copying the unmatched prefix bytes
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", output_write_off)); // reload the current replacement output write cursor before appending bytes
    emitter.instruction("xor ecx, ecx");                                        // start copying the unmatched subject prefix from offset zero

    emitter.label("__rt_preg_replace_pre_linux_x86_64");
    emitter.instruction("cmp rcx, r9");                                         // stop copying the unmatched prefix once every byte before rm_so has been emitted
    emitter.instruction("jge __rt_preg_replace_repl_linux_x86_64");             // move on to appending the replacement literal once the unmatched prefix is copied
    emitter.instruction("mov r8b, BYTE PTR [r10 + rcx]");                       // load one unmatched subject byte that precedes the current regex match
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // append the unmatched subject byte into the replacement output buffer
    emitter.instruction("add r11, 1");                                          // advance the replacement output write cursor after appending one prefix byte
    emitter.instruction("add rcx, 1");                                          // advance the unmatched-prefix byte index
    emitter.instruction("jmp __rt_preg_replace_pre_linux_x86_64");              // continue copying the remaining unmatched prefix bytes

    emitter.label("__rt_preg_replace_repl_linux_x86_64");
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", replacement_ptr_off)); // reload the elephc replacement pointer before appending the replacement literal bytes
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", replacement_len_off)); // reload the elephc replacement length before appending the replacement literal bytes
    emitter.instruction("xor ecx, ecx");                                        // start copying the replacement literal from offset zero

    emitter.label("__rt_preg_replace_repl_copy_linux_x86_64");
    emitter.instruction("cmp rcx, rdx");                                        // stop copying once the full replacement literal has been appended
    emitter.instruction("jge __rt_preg_replace_advance_linux_x86_64");          // move on to advancing the current subject cursor past the regex match
    emitter.instruction("mov r8b, BYTE PTR [rax + rcx]");                       // load one replacement literal byte from the elephc replacement string payload
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // append the replacement literal byte into the replacement output buffer
    emitter.instruction("add r11, 1");                                          // advance the replacement output write cursor after appending one literal byte
    emitter.instruction("add rcx, 1");                                          // advance the replacement literal byte index
    emitter.instruction("jmp __rt_preg_replace_repl_copy_linux_x86_64");        // continue copying the remaining replacement literal bytes

    emitter.label("__rt_preg_replace_advance_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", output_write_off)); // preserve the updated replacement output write cursor before advancing the subject cursor
    emitter.instruction(&load_rm_eo);                                           // load rm_eo from the native Linux regmatch_t layout using the correct regoff_t width
    emitter.instruction("cmp r9, 0");                                           // detect zero-length regex matches so the replacement loop still makes forward progress
    emitter.instruction("jg __rt_preg_replace_advance_ok_linux_x86_64");        // trust rm_eo directly when the regex consumed at least one byte
    emitter.instruction("mov r9, 1");                                           // force zero-length matches to advance by one byte and avoid infinite replacement loops
    emitter.label("__rt_preg_replace_advance_ok_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_pos_off)); // reload the current subject cursor before advancing it past the latest regex match
    emitter.instruction("add r10, r9");                                         // advance the current subject cursor by rm_eo bytes or the forced one-byte fallback
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", current_pos_off)); // preserve the advanced subject cursor for the next replacement-loop iteration
    emitter.instruction("jmp __rt_preg_replace_loop_linux_x86_64");             // continue replacing the remaining regex matches in the subject payload

    emitter.label("__rt_preg_replace_tail_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_pos_off)); // reload the current subject cursor before copying the unmatched tail bytes
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", output_write_off)); // reload the replacement output write cursor before appending the unmatched tail

    emitter.label("__rt_preg_replace_tail_loop_linux_x86_64");
    emitter.instruction("mov r8b, BYTE PTR [r10]");                             // load the next unmatched tail byte from the current subject cursor
    emitter.instruction("test r8b, r8b");                                       // stop once the tail copy reaches the subject's trailing null terminator
    emitter.instruction("jz __rt_preg_replace_done_linux_x86_64");              // finish replacement after copying the final unmatched tail byte
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // append the unmatched tail byte into the replacement output buffer
    emitter.instruction("add r10, 1");                                          // advance the current subject cursor to the next tail byte
    emitter.instruction("add r11, 1");                                          // advance the replacement output write cursor after appending one tail byte
    emitter.instruction("jmp __rt_preg_replace_tail_loop_linux_x86_64");        // continue copying the unmatched tail bytes after the final regex match

    emitter.label("__rt_preg_replace_done_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", output_write_off)); // preserve the final replacement output write cursor before finalizing the string result
    emitter.instruction("lea rdi, [rsp]");                                      // reload the compiled regex_t storage before freeing it with regfree()
    emitter.bl_c("regfree");                                                    // release the compiled POSIX regex resources before returning the replacement result
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", output_start_off)); // reload the replacement output start pointer from the concat scratch buffer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", output_write_off)); // reload the final replacement output write cursor before computing the string length
    emitter.instruction("sub rdx, rax");                                        // compute the replacement output length from the output start and final write cursor
    abi::emit_symbol_address(emitter, "r9", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // reload the concat scratch-buffer offset before reserving the emitted replacement bytes
    emitter.instruction("add r10, rdx");                                        // extend the concat scratch-buffer offset by the length of the replacement result
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the updated concat scratch-buffer offset for later string-producing helpers
    emitter.instruction("jmp __rt_preg_replace_ret_linux_x86_64");              // share the common epilogue after materializing the replacement string result

    emitter.label("__rt_preg_replace_fail_linux_x86_64");
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // return the original elephc subject pointer when regex compilation fails
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // return the original elephc subject length when regex compilation fails

    emitter.label("__rt_preg_replace_ret_linux_x86_64");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release the local regex_t, regmatch_t, and replacement spill storage before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the regex-replace helper completes
    emitter.instruction("ret");                                                 // return the preg_replace() string result in rax/rdx
}
