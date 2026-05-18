//! Purpose:
//! Emits the `__rt_preg_replace_callback` runtime helper for regex callbacks.
//! Builds match arrays, invokes the callback, and appends returned strings.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - Callback strings may use `_concat_buf`; the helper delays copying each
//!   unmatched prefix until after callback results have been persisted, and
//!   backs up already-emitted output because callback prologues reset `_concat_off`.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

const PREG_REPLACE_CALLBACK_NMATCH: usize = 10;

/// __rt_preg_replace_callback: replace regex matches with a callback result.
/// Input:  x1=pattern ptr, x2=pattern len, x3=callback ptr, x4=callback env ptr,
///         x5=subject ptr, x6=subject len
/// Output: x1=result ptr, x2=result len
pub(crate) fn emit_preg_replace_callback(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_replace_callback_linux_x86_64(emitter);
        return;
    }

    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let regmatches_size = emitter.platform.regmatch_t_size() * PREG_REPLACE_CALLBACK_NMATCH;
    let pattern_ptr_off = regmatch_off + regmatches_size;
    let pattern_len_off = pattern_ptr_off + 8;
    let callback_ptr_off = pattern_len_off + 8;
    let callback_env_off = callback_ptr_off + 8;
    let subject_ptr_off = callback_env_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let output_start_off = subject_cstr_off + 8;
    let output_write_off = output_start_off + 8;
    let current_pos_off = output_write_off + 8;
    let matches_array_off = current_pos_off + 8;
    let group_idx_off = matches_array_off + 8;
    let prefix_len_off = group_idx_off + 8;
    let output_backup_ptr_off = prefix_len_off + 8;
    let output_backup_len_off = output_backup_ptr_off + 8;
    let callback_result_ptr_off = output_backup_len_off + 8;
    let callback_result_len_off = callback_result_ptr_off + 8;
    let stack_size = (callback_result_len_off + 96 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_replace_callback ---");
    emitter.label_global("__rt_preg_replace_callback");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_replace_callback stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // establish the runtime helper frame pointer

    // -- save all inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // save pattern pointer for delimiter stripping
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // save pattern length for delimiter stripping
    emitter.instruction(&format!("str x3, [sp, #{}]", callback_ptr_off));       // save callback entry point across libc calls
    emitter.instruction(&format!("str x4, [sp, #{}]", callback_env_off));       // save optional callback capture environment
    emitter.instruction(&format!("str x5, [sp, #{}]", subject_ptr_off));        // save subject pointer for fallback and C-string conversion
    emitter.instruction(&format!("str x6, [sp, #{}]", subject_len_off));        // save subject length for fallback and C-string conversion

    // -- strip delimiters and compile the translated POSIX regex --
    emitter.instruction("bl __rt_preg_strip");                                  // strip slash delimiters and expose supported regex flags
    emitter.instruction(&format!("str x3, [sp, #{}]", flags_off));              // save regex flags from the stripped pattern
    emitter.instruction("bl __rt_pcre_to_posix");                               // translate common PCRE shorthand into a POSIX C string
    emitter.instruction(&format!("str x0, [sp, #{}]", pattern_cstr_off));       // save translated pattern C string
    emitter.instruction("mov x0, sp");                                          // pass regex_t storage at the bottom of this frame
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_cstr_off));       // pass translated pattern C string to regcomp
    emitter.instruction("mov x2, #1");                                          // start with REG_EXTENDED
    emitter.instruction(&format!("ldr x9, [sp, #{}]", flags_off));              // reload supported regex flags
    emitter.instruction("tst x9, #1");                                          // test whether the pattern requested case-insensitive matching
    emitter.instruction("b.eq __rt_preg_replace_callback_nc");                  // keep default flags when /i was not present
    emitter.instruction("orr x2, x2, #2");                                      // add REG_ICASE for case-insensitive matching
    emitter.label("__rt_preg_replace_callback_nc");
    emitter.bl_c("regcomp");                                                    // compile the translated regex through libc
    emitter.instruction("cbnz x0, __rt_preg_replace_callback_fail");            // return the original subject when regex compilation fails

    // -- materialize subject as a C string for repeated regexec calls --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // reload subject pointer for C-string conversion
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // reload subject length for C-string conversion
    emitter.instruction("bl __rt_cstr2");                                       // copy subject to the secondary null-terminated buffer
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save null-terminated subject pointer

    // -- set up output buffer in concat_buf --
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current concat-buffer offset
    abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // compute the replacement output start pointer
    emitter.instruction(&format!("str x11, [sp, #{}]", output_start_off));      // save final output start pointer
    emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off));      // initialize final output write pointer
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_cstr_off));       // load subject C-string start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_pos_off));        // initialize current regex search cursor

    // -- replacement loop --
    emitter.label("__rt_preg_replace_callback_loop");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_pos_off));        // load current subject cursor
    emitter.instruction("ldrb w9, [x1]");                                       // read the current subject byte
    emitter.instruction("cbz w9, __rt_preg_replace_callback_done");             // finish when the cursor reaches the null terminator
    emitter.instruction("mov x0, sp");                                          // pass regex_t storage to regexec
    emitter.instruction(&format!("mov x2, #{}", PREG_REPLACE_CALLBACK_NMATCH)); // request full match plus the supported capture slots
    emitter.instruction(&format!("add x3, sp, #{}", regmatch_off));             // pass regmatch_t capture buffer
    emitter.instruction("mov x4, #0");                                          // use default regexec execution flags
    emitter.bl_c("regexec");                                                    // execute regex at the current subject cursor
    emitter.instruction("cbnz x0, __rt_preg_replace_callback_tail");            // copy the remaining subject when there are no more matches

    // -- remember unmatched prefix length before this match --
    emitter.instruction(&emitter.platform.regoff_load_instr("x9", "sp", regmatch_off)); // load rm_so with the platform regoff_t width
    emitter.instruction(&format!("str x9, [sp, #{}]", prefix_len_off));         // save prefix byte count until callback scratch work is done

    // -- build callback matches array from capture slots --
    emitter.label("__rt_preg_replace_callback_matches");
    emitter.instruction(&format!("mov x0, #{}", PREG_REPLACE_CALLBACK_NMATCH)); // allocate enough match slots for full match plus captures
    emitter.instruction("mov x1, #16");                                         // string array slots store pointer and length pairs
    emitter.instruction("bl __rt_array_new");                                   // allocate indexed string matches array
    emitter.instruction(&format!("str x0, [sp, #{}]", matches_array_off));      // save matches array pointer across pushes
    emitter.instruction("mov x12, #0");                                         // start with $matches[0]
    emitter.instruction(&format!("str x12, [sp, #{}]", group_idx_off));         // save current capture index
    emitter.label("__rt_preg_replace_callback_group_loop");
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload current capture index
    emitter.instruction(&format!("cmp x12, #{}", PREG_REPLACE_CALLBACK_NMATCH)); // stop after the supported capture window
    emitter.instruction("b.ge __rt_preg_replace_callback_call");                // invoke callback after all supported captures are materialized
    if emitter.platform.regmatch_t_size() == 16 {
        emitter.instruction("lsl x14, x12, #4");                                // scale capture index by native regmatch_t size
        emitter.instruction(&format!("add x14, x14, #{}", regmatch_off));       // add capture buffer base offset
        emitter.instruction("ldr x15, [sp, x14]");                              // load capture start offset
        emitter.instruction(&format!("add x14, x14, #{}", emitter.platform.regmatch_rm_eo_offset())); // advance to capture end offset
        emitter.instruction("ldr x16, [sp, x14]");                              // load capture end offset
    } else {
        emitter.instruction("lsl x14, x12, #3");                                // scale capture index by compact regmatch_t size
        emitter.instruction(&format!("add x14, x14, #{}", regmatch_off));       // add capture buffer base offset
        emitter.instruction("ldrsw x15, [sp, x14]");                            // load capture start offset
        emitter.instruction(&format!("add x14, x14, #{}", emitter.platform.regmatch_rm_eo_offset())); // advance to capture end offset
        emitter.instruction("ldrsw x16, [sp, x14]");                            // load capture end offset
    }
    emitter.instruction("cmp x15, #0");                                         // check whether this capture was populated
    emitter.instruction("b.lt __rt_preg_replace_callback_call");                // stop at the first missing capture slot
    emitter.instruction("sub x2, x16, x15");                                    // compute capture byte length
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_pos_off));        // reload current subject cursor
    emitter.instruction("add x1, x1, x15");                                     // compute capture byte pointer
    emitter.instruction(&format!("ldr x0, [sp, #{}]", matches_array_off));      // reload matches array pointer
    emitter.instruction("bl __rt_array_push_str");                              // append owned capture string to matches array
    emitter.instruction(&format!("str x0, [sp, #{}]", matches_array_off));      // save possibly grown matches array pointer
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload current capture index after helper call
    emitter.instruction("add x12, x12, #1");                                    // advance to next capture slot
    emitter.instruction(&format!("str x12, [sp, #{}]", group_idx_off));         // save next capture index
    emitter.instruction("b __rt_preg_replace_callback_group_loop");             // continue materializing capture strings

    // -- invoke callback and append its string result --
    emitter.label("__rt_preg_replace_callback_call");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", output_start_off));       // load current output start before callback scratch can overwrite it
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // load current output write pointer before callback invocation
    emitter.instruction("sub x2, x11, x1");                                     // compute bytes already emitted before the current replacement
    emitter.instruction("bl __rt_str_persist");                                 // back up already-emitted output outside callback scratch space
    emitter.instruction(&format!("str x1, [sp, #{}]", output_backup_ptr_off));  // save output backup pointer across callback invocation
    emitter.instruction(&format!("str x2, [sp, #{}]", output_backup_len_off));  // save output backup length across callback invocation
    publish_concat_offset(emitter, output_write_off);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", matches_array_off));      // pass matches array as the callback's first argument
    emitter.instruction(&format!("ldr x9, [sp, #{}]", callback_env_off));       // load optional callback capture environment
    emitter.instruction("cbz x9, __rt_preg_replace_callback_direct");           // omit env argument for direct callbacks
    emitter.instruction("mov x1, x9");                                          // pass capture environment after visible callback args
    emitter.label("__rt_preg_replace_callback_direct");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", callback_ptr_off));      // reload callback entry point
    emitter.instruction("blr x10");                                             // call callback and receive replacement string in x1/x2
    emitter.instruction("bl __rt_str_persist");                                 // copy callback result away from volatile concat-buffer scratch space
    emitter.instruction(&format!("str x1, [sp, #{}]", callback_result_ptr_off)); // save persisted callback result pointer across prefix copying
    emitter.instruction(&format!("str x2, [sp, #{}]", callback_result_len_off)); // save persisted callback result length across prefix copying

    // -- restore output already emitted before the callback clobbered concat_buf --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", output_backup_ptr_off));  // reload backed-up output prefix pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", output_backup_len_off));  // reload backed-up output prefix length
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_start_off));      // reload final output start for prefix restoration
    emitter.instruction("mov x12, #0");                                         // initialize output restoration index
    emitter.label("__rt_preg_replace_callback_restore_output");
    emitter.instruction("cmp x12, x2");                                         // check whether all previous output bytes have been restored
    emitter.instruction("b.ge __rt_preg_replace_callback_restore_done");        // continue once the pre-callback output is back in place
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load one backed-up output byte
    emitter.instruction("strb w13, [x11, x12]");                                // restore the output byte to its original concat-buffer slot
    emitter.instruction("add x12, x12, #1");                                    // advance the output restoration index
    emitter.instruction("b __rt_preg_replace_callback_restore_output");         // keep restoring previously emitted output bytes
    emitter.label("__rt_preg_replace_callback_restore_done");
    emitter.instruction("add x11, x11, x2");                                    // resume appending at the end of the restored output

    // -- copy unmatched prefix after callback scratch has been persisted --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", prefix_len_off));         // reload unmatched prefix byte count
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // reload current subject cursor for prefix copy
    emitter.instruction("mov x12, #0");                                         // initialize prefix copy index
    emitter.label("__rt_preg_replace_callback_pre");
    emitter.instruction("cmp x12, x9");                                         // compare prefix copy index with rm_so
    emitter.instruction("b.ge __rt_preg_replace_callback_copy_repl_start");     // switch to callback result once the prefix is copied
    emitter.instruction("ldrb w13, [x10, x12]");                                // load next unmatched prefix byte
    emitter.instruction("strb w13, [x11]");                                     // append unmatched prefix byte to output
    emitter.instruction("add x11, x11, #1");                                    // advance output write pointer
    emitter.instruction("add x12, x12, #1");                                    // advance prefix copy index
    emitter.instruction("b __rt_preg_replace_callback_pre");                    // keep copying unmatched prefix bytes

    // -- append callback string result --
    emitter.label("__rt_preg_replace_callback_copy_repl_start");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", callback_result_ptr_off)); // reload persisted callback result pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", callback_result_len_off)); // reload persisted callback result length
    emitter.instruction("mov x12, #0");                                         // initialize callback-result copy index
    emitter.label("__rt_preg_replace_callback_copy_repl");
    emitter.instruction("cmp x12, x2");                                         // compare copied bytes against callback result length
    emitter.instruction("b.ge __rt_preg_replace_callback_advance");             // advance regex cursor when callback result is fully copied
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load next callback result byte
    emitter.instruction("strb w13, [x11]");                                     // append callback result byte to output
    emitter.instruction("add x11, x11, #1");                                    // advance output write pointer
    emitter.instruction("add x12, x12, #1");                                    // advance callback-result copy index
    emitter.instruction("b __rt_preg_replace_callback_copy_repl");              // continue copying callback result bytes

    // -- advance past this match --
    emitter.label("__rt_preg_replace_callback_advance");
    emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off));      // save output write pointer after callback copy
    publish_concat_offset(emitter, output_write_off);
    emitter.instruction(&emitter.platform.regoff_load_instr(
        "x9",
        "sp",
        regmatch_off + emitter.platform.regmatch_rm_eo_offset(),
    ));                                                                         // load rm_eo with the platform regoff_t width
    emitter.instruction("cmp x9, #0");                                          // detect zero-length regex matches
    emitter.instruction("b.gt __rt_preg_replace_callback_advance_ok");          // use native rm_eo when the match consumed bytes
    emitter.instruction("mov x9, #1");                                          // force progress for zero-length matches
    emitter.label("__rt_preg_replace_callback_advance_ok");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // reload current subject cursor
    emitter.instruction("add x10, x10, x9");                                    // move cursor past the current match
    emitter.instruction(&format!("str x10, [sp, #{}]", current_pos_off));       // save next regex search cursor
    emitter.instruction("b __rt_preg_replace_callback_loop");                   // continue replacing further matches

    // -- copy trailing subject after the last match --
    emitter.label("__rt_preg_replace_callback_tail");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // reload current subject cursor for tail copy
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // reload output write pointer for tail copy
    emitter.label("__rt_preg_replace_callback_tail_loop");
    emitter.instruction("ldrb w9, [x10]");                                      // load next tail byte
    emitter.instruction("cbz w9, __rt_preg_replace_callback_done");             // finish when tail reaches the null terminator
    emitter.instruction("strb w9, [x11]");                                      // append tail byte to output
    emitter.instruction("add x10, x10, #1");                                    // advance tail source pointer
    emitter.instruction("add x11, x11, #1");                                    // advance output write pointer
    emitter.instruction("b __rt_preg_replace_callback_tail_loop");              // continue copying tail bytes

    // -- free regex and return final output slice --
    emitter.label("__rt_preg_replace_callback_done");
    emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off));      // save final output pointer
    emitter.instruction("mov x0, sp");                                          // pass regex_t storage to regfree
    emitter.bl_c("regfree");                                                    // release compiled regex resources
    emitter.instruction(&format!("ldr x1, [sp, #{}]", output_start_off));       // return output start pointer
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // reload output end pointer
    emitter.instruction("sub x2, x11, x1");                                     // compute output byte length
    publish_concat_offset(emitter, output_write_off);
    emitter.instruction("b __rt_preg_replace_callback_ret");                    // share common epilogue

    // -- failure: return original subject --
    emitter.label("__rt_preg_replace_callback_fail");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // return original subject pointer on regex compilation failure
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // return original subject length on regex compilation failure

    emitter.label("__rt_preg_replace_callback_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // release preg_replace_callback stack frame
    emitter.instruction("ret");                                                 // return to generated code
}

fn publish_concat_offset(emitter: &mut Emitter, output_write_off: usize) {
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // reload current output write pointer for concat publication
    abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("sub x10, x11, x9");                                    // compute current absolute concat-buffer offset
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x10, [x9]");                                       // publish concat offset before a nested callback writes strings
}

fn emit_preg_replace_callback_linux_x86_64(emitter: &mut Emitter) {
    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_rm_eo_off = emitter.platform.regmatch_rm_eo_offset();
    let regmatch_off = regex_t_size;
    let regmatches_size = emitter.platform.regmatch_t_size() * PREG_REPLACE_CALLBACK_NMATCH;
    let pattern_ptr_off = regmatch_off + regmatches_size;
    let pattern_len_off = pattern_ptr_off + 8;
    let callback_ptr_off = pattern_len_off + 8;
    let callback_env_off = callback_ptr_off + 8;
    let subject_ptr_off = callback_env_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let output_start_off = subject_cstr_off + 8;
    let output_write_off = output_start_off + 8;
    let current_pos_off = output_write_off + 8;
    let matches_array_off = current_pos_off + 8;
    let group_idx_off = matches_array_off + 8;
    let prefix_len_off = group_idx_off + 8;
    let output_backup_ptr_off = prefix_len_off + 8;
    let output_backup_len_off = output_backup_ptr_off + 8;
    let callback_result_ptr_off = output_backup_len_off + 8;
    let callback_result_len_off = callback_result_ptr_off + 8;
    let stack_size = (callback_result_len_off + 16 + 15) & !15;
    let load_rm_so = if emitter.platform.regmatch_t_size() == 16 {
        format!("mov r9, QWORD PTR [rsp + {}]", regmatch_off)
    } else {
        format!("movsxd r9, DWORD PTR [rsp + {}]", regmatch_off)
    };
    let load_rm_eo = if emitter.platform.regmatch_t_size() == 16 {
        format!("mov r9, QWORD PTR [rsp + {}]", regmatch_off + regmatch_rm_eo_off)
    } else {
        format!("movsxd r9, DWORD PTR [rsp + {}]", regmatch_off + regmatch_rm_eo_off)
    };

    emitter.blank();
    emitter.comment("--- runtime: preg_replace_callback ---");
    emitter.label_global("__rt_preg_replace_callback");

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving regex callback scratch storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for regex callback spill slots
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve aligned local storage for regex_t, regmatch_t, and callback bookkeeping

    // -- save all inputs --
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdi", pattern_ptr_off)); // preserve pattern pointer across regex helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rsi", pattern_len_off)); // preserve pattern length across regex helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", callback_ptr_off)); // preserve callback entry point across regex helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", callback_env_off)); // preserve optional callback capture environment
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r8", subject_ptr_off)); // preserve subject pointer for fallback and C-string conversion
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", subject_len_off)); // preserve subject length for fallback and C-string conversion

    // -- strip delimiters and compile the translated POSIX regex --
    emitter.instruction("mov rax, rdi");                                        // move pattern pointer into the delimiter-strip helper input register
    emitter.instruction("mov rdx, rsi");                                        // move pattern length into the delimiter-strip helper input register
    emitter.instruction("call __rt_preg_strip");                                // strip slash delimiters and gather supported regex flags
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", flags_off));  // preserve delimiter-strip flags for regcomp
    emitter.instruction("call __rt_pcre_to_posix");                             // translate PCRE shorthands into a POSIX-compatible C string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off)); // preserve translated pattern C string across compilation
    emitter.instruction("lea rdi, [rsp]");                                      // pass local regex_t storage to regcomp
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off)); // pass translated pattern C string to regcomp
    emitter.instruction("mov edx, 1");                                          // start with REG_EXTENDED
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", flags_off));  // reload supported regex flags
    emitter.instruction("test rcx, 1");                                         // check whether the pattern requested case-insensitive matching
    emitter.instruction("jz __rt_preg_replace_callback_nc_linux_x86_64");       // keep default flags when /i was not present
    emitter.instruction("or edx, 2");                                           // add REG_ICASE for case-insensitive matching
    emitter.label("__rt_preg_replace_callback_nc_linux_x86_64");
    emitter.bl_c("regcomp");                                                    // compile the translated regex through libc
    emitter.instruction("test eax, eax");                                       // did regex compilation succeed?
    emitter.instruction("jnz __rt_preg_replace_callback_fail_linux_x86_64");    // return original subject when regex compilation fails

    // -- materialize subject as a C string for repeated regexec calls --
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload subject pointer for C-string conversion
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload subject length for C-string conversion
    emitter.instruction("call __rt_cstr2");                                     // copy subject to a null-terminated buffer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // save null-terminated subject pointer

    // -- set up output buffer in concat_buf --
    abi::emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load current concat-buffer offset
    abi::emit_symbol_address(emitter, "rax", "_concat_buf");
    emitter.instruction("add rax, r11");                                        // compute the replacement output start pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", output_start_off)); // save final output start pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", output_write_off)); // initialize final output write pointer
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_cstr_off)); // load subject C-string start
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", current_pos_off)); // initialize current regex search cursor

    // -- replacement loop --
    emitter.label("__rt_preg_replace_callback_loop_linux_x86_64");
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_pos_off)); // reload current subject cursor for regexec
    emitter.instruction("movzx r9d, BYTE PTR [rsi]");                           // read the current subject byte
    emitter.instruction("test r9d, r9d");                                       // check whether the cursor reached the null terminator
    emitter.instruction("jz __rt_preg_replace_callback_done_linux_x86_64");     // finish when the full subject has been consumed
    emitter.instruction("lea rdi, [rsp]");                                      // pass regex_t storage to regexec
    emitter.instruction(&format!("mov edx, {}", PREG_REPLACE_CALLBACK_NMATCH)); // request full match plus supported capture slots
    emitter.instruction(&format!("lea rcx, [rsp + {}]", regmatch_off));         // pass regmatch_t capture buffer
    emitter.instruction("xor r8d, r8d");                                        // use default regexec execution flags
    emitter.bl_c("regexec");                                                    // execute regex at the current subject cursor
    emitter.instruction("test eax, eax");                                       // did regexec find another match?
    emitter.instruction("jnz __rt_preg_replace_callback_tail_linux_x86_64");    // copy the remaining subject once no more matches exist

    // -- remember unmatched prefix length before this match --
    emitter.instruction(&load_rm_so);                                           // load rm_so from the native Linux regmatch_t layout
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", prefix_len_off)); // save prefix byte count until callback scratch work is done

    // -- build callback matches array from capture slots --
    emitter.instruction(&format!("mov rdi, {}", PREG_REPLACE_CALLBACK_NMATCH)); // allocate enough match slots for full match plus captures
    emitter.instruction("mov rsi, 16");                                         // string array slots store pointer and length pairs
    emitter.instruction("call __rt_array_new");                                 // allocate indexed string matches array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", matches_array_off)); // save matches array pointer across pushes
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", group_idx_off)); // start with $matches[0]
    emitter.label("__rt_preg_replace_callback_group_loop_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", group_idx_off)); // reload current capture index
    emitter.instruction(&format!("cmp r10, {}", PREG_REPLACE_CALLBACK_NMATCH)); // stop after the supported capture window
    emitter.instruction("jge __rt_preg_replace_callback_call_linux_x86_64");    // invoke callback after supported captures are materialized
    if emitter.platform.regmatch_t_size() == 16 {
        emitter.instruction("shl r10, 4");                                      // scale capture index by native regmatch_t size
        emitter.instruction(&format!("add r10, {}", regmatch_off));             // add capture buffer base offset
        emitter.instruction("mov r11, QWORD PTR [rsp + r10]");                  // load capture start offset
        emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + r10 + {}]", regmatch_rm_eo_off)); // load capture end offset
    } else {
        emitter.instruction("shl r10, 3");                                      // scale capture index by compact regmatch_t size
        emitter.instruction(&format!("add r10, {}", regmatch_off));             // add capture buffer base offset
        emitter.instruction("movsxd r11, DWORD PTR [rsp + r10]");               // load capture start offset
        emitter.instruction(&format!("movsxd rcx, DWORD PTR [rsp + r10 + {}]", regmatch_rm_eo_off)); // load capture end offset
    }
    emitter.instruction("cmp r11, 0");                                          // check whether this capture was populated
    emitter.instruction("jl __rt_preg_replace_callback_call_linux_x86_64");     // stop at the first missing capture slot
    emitter.instruction("sub rcx, r11");                                        // compute capture byte length
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", current_pos_off)); // reload current subject cursor
    emitter.instruction("add rsi, r11");                                        // compute capture byte pointer
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", matches_array_off)); // reload matches array pointer
    emitter.instruction("mov rdx, rcx");                                        // pass capture byte length to array_push_str
    emitter.instruction("call __rt_array_push_str");                            // append owned capture string to matches array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", matches_array_off)); // save possibly grown matches array pointer
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", group_idx_off)); // reload current capture index after helper call
    emitter.instruction("add r10, 1");                                          // advance to next capture slot
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", group_idx_off)); // save next capture index
    emitter.instruction("jmp __rt_preg_replace_callback_group_loop_linux_x86_64"); // continue materializing capture strings

    // -- invoke callback and append its string result --
    emitter.label("__rt_preg_replace_callback_call_linux_x86_64");
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", output_start_off)); // load current output start before callback scratch can overwrite it
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", output_write_off)); // load current output write pointer before callback invocation
    emitter.instruction("mov rdx, r11");                                        // copy output write pointer for emitted-byte calculation
    emitter.instruction("sub rdx, rax");                                        // compute bytes already emitted before the current replacement
    emitter.instruction("call __rt_str_persist");                               // back up already-emitted output outside callback scratch space
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", output_backup_ptr_off)); // save output backup pointer across callback invocation
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", output_backup_len_off)); // save output backup length across callback invocation
    publish_concat_offset_x86_64(emitter, output_write_off);
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", matches_array_off)); // pass matches array as the callback's first argument
    emitter.instruction(&format!("cmp QWORD PTR [rsp + {}], 0", callback_env_off)); // check whether callback has a capture environment
    emitter.instruction("je __rt_preg_replace_callback_direct_linux_x86_64");   // omit env argument for direct callbacks
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", callback_env_off)); // pass capture environment after visible callback args
    emitter.label("__rt_preg_replace_callback_direct_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", callback_ptr_off)); // reload callback entry point
    emitter.instruction("call r10");                                            // call callback and receive replacement string in rax/rdx
    emitter.instruction("call __rt_str_persist");                               // copy callback result away from volatile concat-buffer scratch space
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", callback_result_ptr_off)); // save persisted callback result pointer across prefix copying
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", callback_result_len_off)); // save persisted callback result length across prefix copying

    // -- restore output already emitted before the callback clobbered concat_buf --
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", output_backup_ptr_off)); // reload backed-up output prefix pointer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", output_backup_len_off)); // reload backed-up output prefix length
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", output_start_off)); // reload final output start for prefix restoration
    emitter.instruction("xor ecx, ecx");                                        // initialize output restoration index
    emitter.label("__rt_preg_replace_callback_restore_output_linux_x86_64");
    emitter.instruction("cmp rcx, rdx");                                        // check whether all previous output bytes have been restored
    emitter.instruction("jge __rt_preg_replace_callback_restore_done_linux_x86_64"); // continue once pre-callback output is back in place
    emitter.instruction("mov r8b, BYTE PTR [rax + rcx]");                       // load one backed-up output byte
    emitter.instruction("mov BYTE PTR [r11 + rcx], r8b");                       // restore the output byte to its original concat-buffer slot
    emitter.instruction("add rcx, 1");                                          // advance the output restoration index
    emitter.instruction("jmp __rt_preg_replace_callback_restore_output_linux_x86_64"); // keep restoring previously emitted output bytes
    emitter.label("__rt_preg_replace_callback_restore_done_linux_x86_64");
    emitter.instruction("add r11, rdx");                                        // resume appending at the end of the restored output

    // -- copy unmatched prefix after callback scratch has been persisted --
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", prefix_len_off)); // reload unmatched prefix byte count
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_pos_off)); // reload current subject cursor for prefix copy
    emitter.instruction("xor ecx, ecx");                                        // initialize prefix copy index
    emitter.label("__rt_preg_replace_callback_pre_linux_x86_64");
    emitter.instruction("cmp rcx, r9");                                         // compare prefix copy index with rm_so
    emitter.instruction("jge __rt_preg_replace_callback_copy_repl_start_linux_x86_64"); // switch to callback result once prefix is copied
    emitter.instruction("mov r8b, BYTE PTR [r10 + rcx]");                       // load next unmatched prefix byte
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // append unmatched prefix byte to output
    emitter.instruction("add r11, 1");                                          // advance output write pointer
    emitter.instruction("add rcx, 1");                                          // advance prefix copy index
    emitter.instruction("jmp __rt_preg_replace_callback_pre_linux_x86_64");     // keep copying unmatched prefix bytes

    // -- append callback string result --
    emitter.label("__rt_preg_replace_callback_copy_repl_start_linux_x86_64");
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", callback_result_ptr_off)); // reload persisted callback result pointer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", callback_result_len_off)); // reload persisted callback result length
    emitter.instruction("xor ecx, ecx");                                        // initialize callback-result copy index
    emitter.label("__rt_preg_replace_callback_copy_repl_linux_x86_64");
    emitter.instruction("cmp rcx, rdx");                                        // compare copied bytes against callback result length
    emitter.instruction("jge __rt_preg_replace_callback_advance_linux_x86_64"); // advance regex cursor when callback result is fully copied
    emitter.instruction("mov r8b, BYTE PTR [rax + rcx]");                       // load next callback result byte
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // append callback result byte to output
    emitter.instruction("add r11, 1");                                          // advance output write pointer
    emitter.instruction("add rcx, 1");                                          // advance callback-result copy index
    emitter.instruction("jmp __rt_preg_replace_callback_copy_repl_linux_x86_64"); // continue copying callback result bytes

    // -- advance past this match --
    emitter.label("__rt_preg_replace_callback_advance_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", output_write_off)); // save output write pointer after callback copy
    publish_concat_offset_x86_64(emitter, output_write_off);
    emitter.instruction(&load_rm_eo);                                           // load rm_eo from the native Linux regmatch_t layout
    emitter.instruction("cmp r9, 0");                                           // detect zero-length regex matches
    emitter.instruction("jg __rt_preg_replace_callback_advance_ok_linux_x86_64"); // use native rm_eo when the match consumed bytes
    emitter.instruction("mov r9, 1");                                           // force progress for zero-length matches
    emitter.label("__rt_preg_replace_callback_advance_ok_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_pos_off)); // reload current subject cursor
    emitter.instruction("add r10, r9");                                         // move cursor past the current match
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", current_pos_off)); // save next regex search cursor
    emitter.instruction("jmp __rt_preg_replace_callback_loop_linux_x86_64");    // continue replacing further matches

    // -- copy trailing subject after the last match --
    emitter.label("__rt_preg_replace_callback_tail_linux_x86_64");
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", current_pos_off)); // reload current subject cursor for tail copy
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", output_write_off)); // reload output write pointer for tail copy
    emitter.label("__rt_preg_replace_callback_tail_loop_linux_x86_64");
    emitter.instruction("mov r8b, BYTE PTR [r10]");                             // load next tail byte
    emitter.instruction("test r8b, r8b");                                       // check whether tail reached the null terminator
    emitter.instruction("jz __rt_preg_replace_callback_done_linux_x86_64");     // finish when tail reaches the null terminator
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // append tail byte to output
    emitter.instruction("add r10, 1");                                          // advance tail source pointer
    emitter.instruction("add r11, 1");                                          // advance output write pointer
    emitter.instruction("jmp __rt_preg_replace_callback_tail_loop_linux_x86_64"); // continue copying tail bytes

    // -- free regex and return final output slice --
    emitter.label("__rt_preg_replace_callback_done_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", output_write_off)); // save final output pointer
    emitter.instruction("lea rdi, [rsp]");                                      // pass regex_t storage to regfree
    emitter.bl_c("regfree");                                                    // release compiled regex resources
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", output_start_off)); // return output start pointer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", output_write_off)); // reload output end pointer
    emitter.instruction("sub rdx, rax");                                        // compute output byte length
    publish_concat_offset_x86_64(emitter, output_write_off);
    emitter.instruction("jmp __rt_preg_replace_callback_ret_linux_x86_64");     // share common epilogue

    // -- failure: return original subject --
    emitter.label("__rt_preg_replace_callback_fail_linux_x86_64");
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // return original subject pointer on regex compilation failure
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // return original subject length on regex compilation failure

    emitter.label("__rt_preg_replace_callback_ret_linux_x86_64");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release preg_replace_callback stack frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code
}

fn publish_concat_offset_x86_64(emitter: &mut Emitter, output_write_off: usize) {
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", output_write_off)); // reload current output write pointer for concat publication
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("mov r10, r11");                                        // copy output pointer before converting it into an absolute offset
    emitter.instruction("sub r10, r9");                                         // compute current absolute concat-buffer offset
    abi::emit_symbol_address(emitter, "r9", "_concat_off");
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish concat offset before a nested callback writes strings
}
