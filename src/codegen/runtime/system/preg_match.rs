//! Purpose:
//! Emits the `__rt_preg_match`, `__rt_preg_strip` runtime helper assembly for preg match.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - Regex helpers preserve PHP PCRE-flavored inputs for PCRE2 and must preserve match array construction.

use crate::codegen::{emit::Emitter, platform::Arch};

/// __rt_preg_match: check if a PCRE regex matches a subject string.
/// Input:  x1=pattern ptr, x2=pattern len, x3=subject ptr, x4=subject len
/// Output: x0=1 if match found, 0 if not
pub(crate) fn emit_preg_match(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_match_linux_x86_64(emitter);
        emit_preg_match_capture_linux_x86_64(emitter);
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

    // -- materialize the PCRE pattern as a C string --
    emitter.instruction("bl __rt_pcre_to_posix");                               // materialize PCRE pattern as a C string
    emitter.instruction(&format!("str x0, [sp, #{}]", pattern_cstr_off));       // save pattern C string

    // -- prepare locale state for regex helpers --
    super::emit_prepare_regex_locale(emitter);

    // -- compile regex: pcre2_regcomp(&regex_t, pattern, flags) --
    emitter.instruction("mov x0, sp");                                          // x0 = regex_t at sp+0
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_cstr_off));       // x1 = pattern C string
    emitter.instruction(&format!("ldr x2, [sp, #{}]", flags_off));              // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.bl_c("pcre2_regcomp");                                              // compile regex through PCRE2
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
    emitter.bl_c("pcre2_regexec");                                                    // regexec → x0=0 if match
    emitter.instruction(&format!("str x0, [sp, #{}]", regexec_result_off));     // save regexec result

    // -- free compiled regex --
    emitter.instruction("mov x0, sp");                                          // x0 = regex_t
    emitter.bl_c("pcre2_regfree");                                                    // free compiled regex

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

    emit_preg_match_capture_arm64(emitter);
}

/// Emits ARM64 `__rt_preg_match_capture`, returning the match flag and `$matches` array.
fn emit_preg_match_capture_arm64(emitter: &mut Emitter) {
    let regex_t_size = emitter.platform.regex_t_size();
    let regex_re_nsub_off = emitter.platform.regex_re_nsub_offset();
    let regmatch_size = emitter.platform.regmatch_t_size();
    let regmatch_rm_eo_off = emitter.platform.regmatch_rm_eo_offset();
    let regmatches_ptr_off = regex_t_size;
    let nmatch_off = regmatches_ptr_off + 8;
    let pattern_ptr_off = nmatch_off + 8;
    let pattern_len_off = pattern_ptr_off + 8;
    let subject_ptr_off = pattern_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let regexec_result_off = subject_cstr_off + 8;
    let matches_array_off = regexec_result_off + 8;
    let group_idx_off = matches_array_off + 8;
    let max_group_off = group_idx_off + 8;
    let stack_size = (max_group_off + 96 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_match_capture ---");
    emitter.label_global("__rt_preg_match_capture");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_match capture stack frame
    emitter.instruction(&format!("add x9, sp, #{}", save_off));                 // compute save-slot address beyond ARM64 pair-store range
    emitter.instruction("stp x29, x30, [x9]");                                  // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // establish the preg_match capture frame pointer

    // -- save inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // save pattern ptr
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // save pattern len
    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off));        // save subject ptr
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off));        // save subject len

    // -- strip delimiters and compile PCRE regex --
    emitter.instruction("bl __rt_preg_strip");                                  // strip delimiters and expose regex flags
    emitter.instruction(&format!("str x3, [sp, #{}]", flags_off));              // save stripped regex flags
    emitter.instruction("bl __rt_pcre_to_posix");                               // materialize PCRE pattern as a C string
    emitter.instruction(&format!("str x0, [sp, #{}]", pattern_cstr_off));       // save null-terminated PCRE pattern
    super::emit_prepare_regex_locale(emitter);
    emitter.instruction("mov x0, sp");                                          // x0 = regex_t storage
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_cstr_off));       // x1 = null-terminated PCRE pattern
    emitter.instruction(&format!("ldr x2, [sp, #{}]", flags_off));              // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.bl_c("pcre2_regcomp");                                              // compile regex through PCRE2
    emitter.instruction("cbnz x0, __rt_preg_match_capture_empty");              // compile failure produces no match and an empty matches array

    // -- allocate a capture buffer sized from regex_t.re_nsub --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", regex_re_nsub_off));      // load regex_t.re_nsub after successful compilation
    emitter.instruction("add x9, x9, #1");                                      // include the full-match slot in the regmatch count
    emitter.instruction(&format!("str x9, [sp, #{}]", nmatch_off));             // save dynamic regmatch count for loops and array sizing
    if regmatch_size == 16 {
        emitter.instruction("lsl x0, x9, #4");                                  // malloc bytes = nmatch * 16-byte regmatch_t slots
    } else {
        emitter.instruction("lsl x0, x9, #3");                                  // malloc bytes = nmatch * 8-byte regmatch_t slots
    }
    emitter.bl_c("malloc");                                                     // allocate the regmatch_t vector for all capture groups
    emitter.instruction("cbz x0, __rt_preg_match_capture_malloc_fail");         // allocation failure returns no match after freeing regex_t
    emitter.instruction(&format!("str x0, [sp, #{}]", regmatches_ptr_off));     // save dynamic regmatch_t buffer pointer

    // -- null-terminate subject and run regex --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // reload subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // reload subject len
    emitter.instruction("bl __rt_cstr2");                                       // materialize null-terminated subject copy
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string
    emit_preg_match_init_regmatches_arm64(emitter, regmatches_ptr_off, nmatch_off, regmatch_size);
    emitter.instruction("mov x0, sp");                                          // pass regex_t storage to regexec
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_cstr_off));       // pass subject C string to regexec
    emitter.instruction(&format!("ldr x2, [sp, #{}]", nmatch_off));             // request one regmatch slot for every capture group
    emitter.instruction(&format!("ldr x3, [sp, #{}]", regmatches_ptr_off));     // pass dynamic regmatch_t capture buffer
    emitter.instruction("mov x4, #0");                                          // use default regexec flags
    emitter.bl_c("pcre2_regexec");                                                    // execute regex and fill capture offsets
    emitter.instruction(&format!("str x0, [sp, #{}]", regexec_result_off));     // save regexec status across cleanup
    emitter.instruction("mov x0, sp");                                          // reload regex_t storage for regfree
    emitter.bl_c("pcre2_regfree");                                                    // release compiled regex resources
    emitter.instruction(&format!("ldr x0, [sp, #{}]", regexec_result_off));     // reload regexec status
    emitter.instruction("cbnz x0, __rt_preg_match_capture_no_match");           // no match frees capture storage and returns an empty array

    // -- find highest populated capture so trailing unmatched groups are omitted --
    emitter.instruction(&format!("ldr x12, [sp, #{}]", nmatch_off));            // reload the dynamic regmatch count
    emitter.instruction("sub x12, x12, #1");                                    // start scanning from the last compiled capture slot
    emitter.label("__rt_preg_match_capture_scan");
    emitter.instruction("mov x14, x12");                                        // copy capture index before scaling to regmatch offset
    if regmatch_size == 16 {
        emitter.instruction("lsl x14, x14, #4");                                // scale capture index by 16-byte regmatch_t slots
    } else {
        emitter.instruction("lsl x14, x14, #3");                                // scale capture index by compact 8-byte regmatch_t slots
    }
    emitter.instruction(&format!("ldr x15, [sp, #{}]", regmatches_ptr_off));    // load dynamic capture buffer base
    emitter.instruction("add x14, x15, x14");                                   // compute address of this regmatch_t slot
    emit_arm_load_regoff_from_index(emitter, "x13", "x14", 0, regmatch_size);
    emitter.instruction("cmp x13, #0");                                         // check whether this capture participated
    emitter.instruction("b.ge __rt_preg_match_capture_scan_found");             // use this as the last emitted capture
    emitter.instruction("cbz x12, __rt_preg_match_capture_scan_found");         // keep at least the full match slot after a successful regexec
    emitter.instruction("sub x12, x12, #1");                                    // move to the previous capture slot
    emitter.instruction("b __rt_preg_match_capture_scan");                      // continue searching for the highest populated capture
    emitter.label("__rt_preg_match_capture_scan_found");
    emitter.instruction(&format!("str x12, [sp, #{}]", max_group_off));         // save highest capture index to materialize

    // -- allocate and fill matches array --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", nmatch_off));             // allocate enough slots for every compiled capture
    emitter.instruction("mov x1, #16");                                         // string arrays use pointer/length payload slots
    emitter.instruction("bl __rt_array_new");                                   // allocate indexed string matches array
    emitter.instruction(&format!("str x0, [sp, #{}]", matches_array_off));      // save matches array pointer across pushes
    emitter.instruction(&format!("str xzr, [sp, #{}]", group_idx_off));         // start with capture index zero
    emitter.label("__rt_preg_match_capture_group_loop");
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload current capture index
    emitter.instruction(&format!("ldr x13, [sp, #{}]", max_group_off));         // reload highest capture index
    emitter.instruction("cmp x12, x13");                                        // have all required captures been materialized?
    emitter.instruction("b.gt __rt_preg_match_capture_success");                // finish after the highest populated capture
    emitter.instruction("mov x14, x12");                                        // copy capture index before scaling to regmatch offset
    if regmatch_size == 16 {
        emitter.instruction("lsl x14, x14, #4");                                // scale capture index by 16-byte regmatch_t slots
    } else {
        emitter.instruction("lsl x14, x14, #3");                                // scale capture index by compact 8-byte regmatch_t slots
    }
    emitter.instruction(&format!("ldr x17, [sp, #{}]", regmatches_ptr_off));    // load dynamic capture buffer base
    emitter.instruction("add x14, x17, x14");                                   // compute address of this regmatch_t slot
    emit_arm_load_regoff_from_index(emitter, "x15", "x14", 0, regmatch_size);
    emit_arm_load_regoff_from_index(
        emitter,
        "x16",
        "x14",
        regmatch_rm_eo_off,
        regmatch_size,
    );
    emitter.instruction("cmp x15, #0");                                         // detect unmatched captures before the highest populated slot
    emitter.instruction("b.lt __rt_preg_match_capture_empty_string");           // emit PHP's empty string for an interior unmatched capture
    emitter.instruction("sub x2, x16, x15");                                    // capture length = rm_eo - rm_so
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_cstr_off));       // reload subject C string base
    emitter.instruction("add x1, x1, x15");                                     // compute capture string pointer
    emitter.instruction("b __rt_preg_match_capture_push");                      // append this capture string
    emitter.label("__rt_preg_match_capture_empty_string");
    emitter.instruction("mov x1, #0");                                          // empty unmatched capture has a null pointer
    emitter.instruction("mov x2, #0");                                          // empty unmatched capture has zero length
    emitter.label("__rt_preg_match_capture_push");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", matches_array_off));      // reload matches array pointer
    emitter.instruction("bl __rt_array_push_str");                              // append capture string to matches array
    emitter.instruction(&format!("str x0, [sp, #{}]", matches_array_off));      // save possibly-grown matches array pointer
    emitter.instruction(&format!("ldr x12, [sp, #{}]", group_idx_off));         // reload capture index after helper calls
    emitter.instruction("add x12, x12, #1");                                    // advance to next capture index
    emitter.instruction(&format!("str x12, [sp, #{}]", group_idx_off));         // save next capture index
    emitter.instruction("b __rt_preg_match_capture_group_loop");                // continue materializing captures

    emitter.label("__rt_preg_match_capture_success");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", regmatches_ptr_off));     // reload dynamic capture buffer for cleanup
    emitter.bl_c("free");                                                       // free the dynamic regmatch_t vector before returning matches
    emitter.instruction("mov x0, #1");                                          // report that preg_match found a match
    emitter.instruction(&format!("ldr x1, [sp, #{}]", matches_array_off));      // return the matches array pointer in x1
    emitter.instruction("b __rt_preg_match_capture_ret");                       // share helper epilogue

    emitter.label("__rt_preg_match_capture_no_match");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", regmatches_ptr_off));     // reload dynamic capture buffer for the no-match cleanup path
    emitter.bl_c("free");                                                       // free the dynamic regmatch_t vector before returning an empty matches array
    emitter.instruction("b __rt_preg_match_capture_empty");                     // allocate and return the empty matches array

    emitter.label("__rt_preg_match_capture_malloc_fail");
    emitter.instruction("mov x0, sp");                                          // reload regex_t storage after capture-buffer allocation failed
    emitter.bl_c("pcre2_regfree");                                                    // free compiled regex resources before returning no match

    emitter.label("__rt_preg_match_capture_empty");
    emitter.instruction("mov x0, #0");                                          // empty array capacity for no-match or compile-failure paths
    emitter.instruction("mov x1, #16");                                         // empty matches array still has string slot metadata
    emitter.instruction("bl __rt_array_new");                                   // allocate empty indexed string matches array
    emitter.instruction("mov x1, x0");                                          // return empty matches array in x1
    emitter.instruction("mov x0, #0");                                          // report no match

    emitter.label("__rt_preg_match_capture_ret");
    emitter.instruction(&format!("add x9, sp, #{}", save_off));                 // compute save-slot address for epilogue restore
    emitter.instruction("ldp x29, x30, [x9]");                                  // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate preg_match capture stack frame
    emitter.instruction("ret");                                                 // return match flag in x0 and matches array in x1
}

/// Prefills ARM64 regmatch slots with unmatched sentinels before regexec.
fn emit_preg_match_init_regmatches_arm64(
    emitter: &mut Emitter,
    regmatches_ptr_off: usize,
    nmatch_off: usize,
    regmatch_size: usize,
) {
    emitter.instruction("mov x9, #-1");                                         // prepare unmatched sentinel for capture slots
    emitter.instruction(&format!("ldr x10, [sp, #{}]", regmatches_ptr_off));    // load dynamic regmatch_t buffer base
    emitter.instruction(&format!("ldr x11, [sp, #{}]", nmatch_off));            // load dynamic regmatch slot count
    emitter.instruction("mov x12, #0");                                         // initialize regmatch initialization index
    emitter.label("__rt_preg_match_capture_init_loop");
    emitter.instruction("cmp x12, x11");                                        // have all dynamic regmatch slots been initialized?
    emitter.instruction("b.ge __rt_preg_match_capture_init_done");              // stop once every slot has an unmatched sentinel
    emitter.instruction("mov x13, x12");                                        // copy index before scaling to native regmatch_t size
    if regmatch_size == 16 {
        emitter.instruction("lsl x13, x13, #4");                                // scale index by 16-byte regmatch_t slots
    } else {
        emitter.instruction("lsl x13, x13, #3");                                // scale index by compact 8-byte regmatch_t slots
    }
    emitter.instruction("add x13, x10, x13");                                   // compute the current dynamic regmatch slot address
    emitter.instruction("str x9, [x13]");                                       // mark the capture start offset as unmatched before regexec
    emitter.instruction("add x12, x12, #1");                                    // advance to the next capture slot
    emitter.instruction("b __rt_preg_match_capture_init_loop");                 // continue initializing dynamic capture slots
    emitter.label("__rt_preg_match_capture_init_done");
}

/// Loads an ARM64 regoff_t field from a dynamically addressed regmatch slot.
fn emit_arm_load_regoff_from_index(
    emitter: &mut Emitter,
    dst: &str,
    addr: &str,
    field_off: usize,
    regmatch_size: usize,
) {
    if field_off == 0 {
        if regmatch_size == 16 {
            emitter.instruction(&format!("ldr {dst}, [{addr}]"));               // load native 64-bit regoff_t from computed regmatch slot
        } else {
            emitter.instruction(&format!("ldrsw {dst}, [{addr}]"));             // sign-extend native 32-bit regoff_t from computed regmatch slot
        }
    } else if regmatch_size == 16 {
        emitter.instruction(&format!("ldr {dst}, [{addr}, #{}]", field_off));   // load native 64-bit regoff_t field from computed regmatch slot
    } else {
        emitter.instruction(&format!("ldrsw {dst}, [{addr}, #{}]", field_off)); // sign-extend native 32-bit regoff_t field from computed slot
    }
}

/// Emits the x86_64 Linux variant of `__rt_preg_match`.
/// Called from `emit_preg_match` when `target.arch == Arch::X86_64`.
/// Uses System V AMD64 ABI: pattern in rdi/rsi, subject in rdx/rcx, result in eax.
/// Compiles a PCRE2 regex via `pcre2_regcomp`, executes via `pcre2_regexec`, then frees with `pcre2_regfree`.
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
    emitter.instruction("call __rt_pcre_to_posix");                             // materialize PCRE pattern as a null-terminated C string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off)); // preserve the null-terminated PCRE pattern pointer for the upcoming regcomp() call
    super::emit_prepare_regex_locale(emitter);
    emitter.instruction("lea rdi, [rsp]");                                      // pass the local regex_t storage as the first regcomp() argument
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off)); // pass the null-terminated PCRE pattern C string as the second regcomp() argument
    emitter.instruction(&format!("mov edx, DWORD PTR [rsp + {}]", flags_off));  // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.bl_c("pcre2_regcomp");                                              // compile the PCRE pattern into the local regex_t storage
    emitter.instruction("test eax, eax");                                       // did regcomp() succeed and produce a compiled regex object?
    emitter.instruction("jnz __rt_preg_match_no_linux_x86_64");                 // failed regex compilation maps to a PHP false-style no-match result
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload the elephc subject pointer before null-terminating it in the secondary scratch buffer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload the elephc subject length before null-terminating it in the secondary scratch buffer
    emitter.instruction("call __rt_cstr2");                                     // materialize a null-terminated C version of the subject string for PCRE2 regex execution
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // preserve the subject C string pointer for the regexec() call and later cleanup path
    emitter.instruction("lea rdi, [rsp]");                                      // pass the compiled regex_t storage as the first regexec() argument
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", subject_cstr_off)); // pass the null-terminated subject C string as the second regexec() argument
    emitter.instruction("mov edx, 1");                                          // request exactly one regmatch_t capture because preg_match() only needs match/no-match
    emitter.instruction(&format!("lea rcx, [rsp + {}]", regmatch_off));         // pass the local regmatch_t buffer as the match output slot for regexec()
    emitter.instruction("xor r8d, r8d");                                        // pass eflags = 0 so PCRE2 regex execution performs a normal match from the subject start
    emitter.bl_c("pcre2_regexec");                                                    // execute the compiled PCRE2 regex against the null-terminated subject string
    emitter.instruction(&format!("mov DWORD PTR [rsp + {}], eax", regexec_result_off)); // preserve the regexec() result code across the mandatory regfree() cleanup call
    emitter.instruction("lea rdi, [rsp]");                                      // reload the compiled regex_t storage before freeing it with regfree()
    emitter.bl_c("pcre2_regfree");                                                    // release any internal PCRE2 regex resources held by the local regex_t object
    emitter.instruction(&format!("mov eax, DWORD PTR [rsp + {}]", regexec_result_off)); // reload the saved regexec() status code after regfree() clobbered caller-saved registers
    emitter.instruction("test eax, eax");                                       // interpret a zero regexec() result as a successful regex match
    emitter.instruction("jnz __rt_preg_match_no_linux_x86_64");                 // return zero when PCRE2 regex execution reports no match
    emitter.instruction("mov eax, 1");                                          // return one when PCRE2 regex execution reports a successful match
    emitter.instruction("jmp __rt_preg_match_ret_linux_x86_64");                // share the common epilogue after materializing the successful match result

    emitter.label("__rt_preg_match_no_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // return zero for compile failures and subjects that do not match the regex

    emitter.label("__rt_preg_match_ret_linux_x86_64");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release the local regex_t, regmatch_t, and subject spill storage before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the regex helper completes
    emitter.instruction("ret");                                                 // return the preg_match() integer result in the x86_64 integer result register
}

/// Emits x86_64 `__rt_preg_match_capture`, returning the match flag and `$matches` array.
fn emit_preg_match_capture_linux_x86_64(emitter: &mut Emitter) {
    let regex_t_size = emitter.platform.regex_t_size();
    let regex_re_nsub_off = emitter.platform.regex_re_nsub_offset();
    let regmatch_size = emitter.platform.regmatch_t_size();
    let regmatch_rm_eo_off = emitter.platform.regmatch_rm_eo_offset();
    let regmatches_ptr_off = regex_t_size;
    let nmatch_off = regmatches_ptr_off + 8;
    let subject_ptr_off = nmatch_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let regexec_result_off = subject_cstr_off + 8;
    let matches_array_off = regexec_result_off + 8;
    let group_idx_off = matches_array_off + 8;
    let max_group_off = group_idx_off + 8;
    let stack_size = (max_group_off + 32 + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: preg_match_capture ---");
    emitter.label_global("__rt_preg_match_capture");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving capture helper storage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for regex and capture spill slots
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve local storage for regex_t, regmatch buffer, and matches array state
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", subject_ptr_off)); // preserve the elephc subject pointer across pattern helper calls
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", subject_len_off)); // preserve the elephc subject length across pattern helper calls
    emitter.instruction("mov rax, rdi");                                        // move pattern pointer into preg-strip helper input register
    emitter.instruction("mov rdx, rsi");                                        // move pattern length into preg-strip helper input register
    emitter.instruction("call __rt_preg_strip");                                // strip slash delimiters and collect supported regex flags
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", flags_off));  // save stripped regex flags for regcomp
    emitter.instruction("call __rt_pcre_to_posix");                             // materialize PCRE pattern as a C string
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off)); // save null-terminated PCRE pattern for regcomp
    super::emit_prepare_regex_locale(emitter);
    emitter.instruction("lea rdi, [rsp]");                                      // pass local regex_t storage to regcomp
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off)); // pass null-terminated PCRE pattern to regcomp
    emitter.instruction(&format!("mov edx, DWORD PTR [rsp + {}]", flags_off));  // pass PCRE2 POSIX compile flags from delimiter parsing
    emitter.bl_c("pcre2_regcomp");                                              // compile regex through PCRE2
    emitter.instruction("test eax, eax");                                       // did regex compilation succeed?
    emitter.instruction("jnz __rt_preg_match_capture_empty_linux_x86_64");      // compile failure returns no match and an empty matches array
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", regex_re_nsub_off)); // load regex_t.re_nsub after successful compilation
    emitter.instruction("add r9, 1");                                           // include the full-match slot in the regmatch count
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", nmatch_off));  // save dynamic regmatch count for loops and array sizing
    emitter.instruction("mov rdi, r9");                                         // copy nmatch before scaling it to a malloc byte count
    if regmatch_size == 16 {
        emitter.instruction("shl rdi, 4");                                      // malloc bytes = nmatch * 16-byte regmatch_t slots
    } else {
        emitter.instruction("shl rdi, 3");                                      // malloc bytes = nmatch * 8-byte regmatch_t slots
    }
    emitter.bl_c("malloc");                                                     // allocate the regmatch_t vector for all capture groups
    emitter.instruction("test rax, rax");                                       // did malloc return a capture buffer?
    emitter.instruction("jz __rt_preg_match_capture_malloc_fail_linux_x86_64"); // allocation failure frees regex_t and returns no match
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", regmatches_ptr_off)); // save dynamic regmatch_t buffer pointer
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload subject pointer before C-string conversion
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload subject length before C-string conversion
    emitter.instruction("call __rt_cstr2");                                     // materialize null-terminated subject for regexec
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // save subject C string across regexec and pushes
    emit_preg_match_init_regmatches_x86_64(emitter, regmatches_ptr_off, nmatch_off, regmatch_size);
    emitter.instruction("lea rdi, [rsp]");                                      // pass compiled regex_t storage to regexec
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", subject_cstr_off)); // pass subject C string to regexec
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", nmatch_off)); // request one regmatch slot for every capture group
    emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // pass dynamic regmatch_t capture buffer
    emitter.instruction("xor r8d, r8d");                                        // use default regexec execution flags
    emitter.bl_c("pcre2_regexec");                                                    // execute regex and fill capture offsets
    emitter.instruction(&format!("mov DWORD PTR [rsp + {}], eax", regexec_result_off)); // save regexec status across regfree
    emitter.instruction("lea rdi, [rsp]");                                      // reload regex_t storage for regfree
    emitter.bl_c("pcre2_regfree");                                                    // release compiled regex resources
    emitter.instruction(&format!("mov eax, DWORD PTR [rsp + {}]", regexec_result_off)); // reload saved regexec status
    emitter.instruction("test eax, eax");                                       // was there a successful regex match?
    emitter.instruction("jnz __rt_preg_match_capture_no_match_linux_x86_64");   // no match frees capture storage and returns an empty array

    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", nmatch_off));  // reload the dynamic regmatch count
    emitter.instruction("sub r9, 1");                                           // start scanning from the last compiled capture slot
    emitter.label("__rt_preg_match_capture_scan_linux_x86_64");
    emitter.instruction("mov r10, r9");                                         // copy capture index before scaling
    emitter.instruction(&format!("imul r10, {}", regmatch_size));               // scale capture index to native regmatch_t stride
    emitter.instruction(&format!("mov r12, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load dynamic capture buffer base
    emitter.instruction("add r10, r12");                                        // compute address of this regmatch_t slot
    emit_x86_load_regoff_from_index(emitter, "r11", "r10", 0, regmatch_size);
    emitter.instruction("cmp r11, 0");                                          // check whether this capture participated
    emitter.instruction("jge __rt_preg_match_capture_scan_found_linux_x86_64"); // use this as the highest emitted capture
    emitter.instruction("test r9, r9");                                         // have we reached the full-match slot?
    emitter.instruction("jz __rt_preg_match_capture_scan_found_linux_x86_64");  // keep at least the full match after successful regexec
    emitter.instruction("sub r9, 1");                                           // move to the previous capture slot
    emitter.instruction("jmp __rt_preg_match_capture_scan_linux_x86_64");       // continue searching for the highest populated capture
    emitter.label("__rt_preg_match_capture_scan_found_linux_x86_64");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", max_group_off)); // save highest capture index to materialize

    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", nmatch_off)); // allocate enough slots for every compiled capture
    emitter.instruction("mov rsi, 16");                                         // string arrays use pointer/length payload slots
    emitter.instruction("call __rt_array_new");                                 // allocate indexed string matches array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", matches_array_off)); // save matches array pointer across pushes
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", group_idx_off)); // start with capture index zero
    emitter.label("__rt_preg_match_capture_group_loop_linux_x86_64");
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_idx_off)); // reload current capture index
    emitter.instruction(&format!("mov r8, QWORD PTR [rsp + {}]", max_group_off)); // reload highest capture index
    emitter.instruction("cmp r9, r8");                                          // have all required captures been materialized?
    emitter.instruction("jg __rt_preg_match_capture_success_linux_x86_64");     // finish after the highest populated capture
    emitter.instruction("mov r10, r9");                                         // copy capture index before scaling
    emitter.instruction(&format!("imul r10, {}", regmatch_size));               // scale capture index to native regmatch_t stride
    emitter.instruction(&format!("mov r12, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load dynamic capture buffer base
    emitter.instruction("add r10, r12");                                        // compute address of this regmatch_t slot
    emit_x86_load_regoff_from_index(emitter, "r11", "r10", 0, regmatch_size);
    emit_x86_load_regoff_from_index(
        emitter,
        "rcx",
        "r10",
        regmatch_rm_eo_off,
        regmatch_size,
    );
    emitter.instruction("cmp r11, 0");                                          // detect unmatched captures before the highest populated slot
    emitter.instruction("jl __rt_preg_match_capture_empty_string_linux_x86_64"); // emit PHP's empty string for an interior unmatched capture
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", subject_cstr_off)); // reload subject C string base
    emitter.instruction("add rsi, r11");                                        // compute capture string pointer
    emitter.instruction("mov rdx, rcx");                                        // copy capture end offset before subtracting start
    emitter.instruction("sub rdx, r11");                                        // capture length = rm_eo - rm_so
    emitter.instruction("jmp __rt_preg_match_capture_push_linux_x86_64");       // append this capture string
    emitter.label("__rt_preg_match_capture_empty_string_linux_x86_64");
    emitter.instruction("xor esi, esi");                                        // empty unmatched capture has a null pointer
    emitter.instruction("xor edx, edx");                                        // empty unmatched capture has zero length
    emitter.label("__rt_preg_match_capture_push_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", matches_array_off)); // reload matches array pointer
    emitter.instruction("call __rt_array_push_str");                            // append capture string to matches array
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", matches_array_off)); // save possibly-grown matches array pointer
    emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", group_idx_off)); // reload capture index after helper calls
    emitter.instruction("add r9, 1");                                           // advance to next capture index
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", group_idx_off)); // save next capture index
    emitter.instruction("jmp __rt_preg_match_capture_group_loop_linux_x86_64"); // continue materializing captures

    emitter.label("__rt_preg_match_capture_success_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // reload dynamic capture buffer for cleanup
    emitter.bl_c("free");                                                       // free the dynamic regmatch_t vector before returning matches
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", matches_array_off)); // return matches array pointer in rdx
    emitter.instruction("mov eax, 1");                                          // report that preg_match found a match
    emitter.instruction("jmp __rt_preg_match_capture_ret_linux_x86_64");        // share helper epilogue

    emitter.label("__rt_preg_match_capture_no_match_linux_x86_64");
    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // reload dynamic capture buffer for the no-match cleanup path
    emitter.bl_c("free");                                                       // free the dynamic regmatch_t vector before returning an empty matches array
    emitter.instruction("jmp __rt_preg_match_capture_empty_linux_x86_64");      // allocate and return the empty matches array

    emitter.label("__rt_preg_match_capture_malloc_fail_linux_x86_64");
    emitter.instruction("lea rdi, [rsp]");                                      // reload regex_t storage after capture-buffer allocation failed
    emitter.bl_c("pcre2_regfree");                                                    // free compiled regex resources before returning no match

    emitter.label("__rt_preg_match_capture_empty_linux_x86_64");
    emitter.instruction("xor edi, edi");                                        // empty array capacity for no-match or compile-failure paths
    emitter.instruction("mov esi, 16");                                         // empty matches array still uses string slot metadata
    emitter.instruction("call __rt_array_new");                                 // allocate empty indexed string matches array
    emitter.instruction("mov rdx, rax");                                        // return empty matches array pointer in rdx
    emitter.instruction("xor eax, eax");                                        // report no match

    emitter.label("__rt_preg_match_capture_ret_linux_x86_64");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release capture helper local storage
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return match flag in rax and matches array in rdx
}

/// Prefills x86_64 regmatch slots with unmatched sentinels before regexec.
fn emit_preg_match_init_regmatches_x86_64(
    emitter: &mut Emitter,
    regmatches_ptr_off: usize,
    nmatch_off: usize,
    regmatch_size: usize,
) {
    emitter.instruction("mov r9, -1");                                          // prepare unmatched sentinel for capture slots
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", regmatches_ptr_off)); // load dynamic regmatch_t buffer base
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", nmatch_off)); // load dynamic regmatch slot count
    emitter.instruction("xor r12d, r12d");                                      // initialize regmatch initialization index
    emitter.label("__rt_preg_match_capture_init_loop_linux_x86_64");
    emitter.instruction("cmp r12, r11");                                        // have all dynamic regmatch slots been initialized?
    emitter.instruction("jge __rt_preg_match_capture_init_done_linux_x86_64");  // stop once every slot has an unmatched sentinel
    emitter.instruction("mov r13, r12");                                        // copy index before scaling to native regmatch_t size
    emitter.instruction(&format!("imul r13, {}", regmatch_size));               // scale index by the target regmatch_t stride
    emitter.instruction("add r13, r10");                                        // compute the current dynamic regmatch slot address
    emitter.instruction("mov QWORD PTR [r13], r9");                             // mark the capture start offset as unmatched before regexec
    emitter.instruction("add r12, 1");                                          // advance to the next capture slot
    emitter.instruction("jmp __rt_preg_match_capture_init_loop_linux_x86_64");  // continue initializing dynamic capture slots
    emitter.label("__rt_preg_match_capture_init_done_linux_x86_64");
}

/// Loads an x86_64 regoff_t field from a dynamically addressed regmatch slot.
fn emit_x86_load_regoff_from_index(
    emitter: &mut Emitter,
    dst: &str,
    addr_reg: &str,
    field_off: usize,
    regmatch_size: usize,
) {
    let suffix = if field_off == 0 {
        String::new()
    } else {
        format!(" + {field_off}")
    };
    if regmatch_size == 16 {
        emitter.instruction(&format!("mov {dst}, QWORD PTR [{addr_reg}{suffix}]")); // load native 64-bit regoff_t from computed regmatch slot
    } else {
        emitter.instruction(&format!("movsxd {dst}, DWORD PTR [{addr_reg}{suffix}]")); // sign-extend native 32-bit regoff_t from computed slot
    }
}
