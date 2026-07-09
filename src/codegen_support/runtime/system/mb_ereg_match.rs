//! Purpose:
//! Emits `__rt_mb_ereg_match`, the runtime helper for PHP's `mb_ereg_match()`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//! - The `mb_ereg_match()` builtin lowering
//!   (`crate::codegen::lower_inst::builtins::regex::lower_mb_ereg_match`).
//!
//! Key details:
//! - `mb_ereg_match($pattern, $string, $options = null)` is an ANCHORED-AT-START match:
//!   it returns true iff the pattern matches beginning at offset 0 of the subject (verified against PHP 8.5:
//!   `mb_ereg_match('bc','abc')` is false, `mb_ereg_match('ab','abc')` is true). It does NOT
//!   require matching to the end unless the pattern says so (`\z`/`$`).
//! - Unlike `preg_match`, the pattern carries NO delimiters and is a bare POSIX/Perl regex, so we
//!   materialize it verbatim as a C string (no `__rt_preg_strip` / `__rt_pcre_to_posix`). PCRE2's
//!   POSIX wrapper always compiles with Perl semantics, so `\z` and friends work. The optional
//!   options string currently maps `i` to the same PCRE2 POSIX `REG_ICASE` bit used by preg helpers.
//! - Start-anchoring is enforced by checking `regmatch[0].rm_so == 0` after a successful
//!   `pcre2_regexec` (PCRE2 returns the leftmost match, so `rm_so == 0` iff it matches at the start).
//! - Input: x1/x2=pattern, x3/x4=subject, x5/x6=options (AArch64) /
//!   rdi/rsi=pattern, rdx/rcx=subject, r8/r9=options (x86_64). Output: 1 if matched at start.

use crate::codegen_support::{emit::Emitter, platform::Arch};

const PCRE2_POSIX_REG_ICASE: i64 = 1;

/// Emits `__rt_mb_ereg_match(pattern, subject, options) -> bool`, the start-anchored matcher.
pub(crate) fn emit_mb_ereg_match(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mb_ereg_match_linux_x86_64(emitter);
        return;
    }

    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let pattern_ptr_off = regmatch_off + emitter.platform.regmatch_t_size();
    let pattern_len_off = pattern_ptr_off + 8;
    let subject_ptr_off = pattern_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let options_ptr_off = subject_len_off + 8;
    let options_len_off = options_ptr_off + 8;
    let pattern_cstr_off = options_len_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let regexec_result_off = subject_cstr_off + 8;
    let stack_size = (regexec_result_off + 40 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: mb_ereg_match (start-anchored regex) ---");
    emitter.label_global("__rt_mb_ereg_match");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate mb_ereg_match stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // establish the mb_ereg_match frame pointer

    // -- save inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // save pattern pointer
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // save pattern length
    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off));        // save subject pointer
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off));        // save subject length
    emitter.instruction(&format!("str x5, [sp, #{}]", options_ptr_off));        // save options pointer, or zero for null options
    emitter.instruction(&format!("str x6, [sp, #{}]", options_len_off));        // save options length, or zero for null options

    // -- materialize the bare pattern as a null-terminated C string (no delimiters to strip) --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_ptr_off));        // reload pattern pointer for C-string materialization
    emitter.instruction(&format!("ldr x2, [sp, #{}]", pattern_len_off));        // reload pattern length for C-string materialization
    emitter.instruction("bl __rt_cstr2");                                       // materialize pattern C string in x0
    emitter.instruction(&format!("str x0, [sp, #{}]", pattern_cstr_off));       // save pattern C string

    super::emit_prepare_regex_locale(emitter);

    // -- translate supported mb_ereg_match options into PCRE2 POSIX compile flags --
    emitter.instruction("mov x2, #0");                                          // start with default PCRE2 POSIX compile flags
    emitter.instruction(&format!("ldr x10, [sp, #{}]", options_len_off));       // load options length
    emitter.instruction(&format!("ldr x11, [sp, #{}]", options_ptr_off));       // load options pointer
    emitter.instruction("cbz x10, __rt_mb_ereg_match_flags_ready");             // no options bytes means default flags
    emitter.instruction("cbz x11, __rt_mb_ereg_match_flags_ready");             // null options pointer means default flags
    emitter.instruction("add x12, x11, x10");                                   // compute one-past-end options pointer
    emitter.label("__rt_mb_ereg_match_flags_loop");
    emitter.instruction("ldrb w13, [x11], #1");                                 // load one option byte and advance
    emitter.instruction("cmp w13, #105");                                       // is the option byte 'i'?
    emitter.instruction("b.ne __rt_mb_ereg_match_flags_skip_i");                // ignore non-`i` option bytes for now
    emitter.instruction(&format!("orr x2, x2, #{}", PCRE2_POSIX_REG_ICASE));    // add REG_ICASE for case-insensitive matching
    emitter.label("__rt_mb_ereg_match_flags_skip_i");
    emitter.instruction("cmp x11, x12");                                        // have all option bytes been consumed?
    emitter.instruction("b.lt __rt_mb_ereg_match_flags_loop");                  // keep scanning while bytes remain
    emitter.label("__rt_mb_ereg_match_flags_ready");

    // -- compile regex: pcre2_regcomp(&regex_t, pattern, flags) --
    emitter.instruction("mov x0, sp");                                          // pass local regex_t storage as regcomp argument 1
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_cstr_off));       // pass pattern C string as regcomp argument 2
    emitter.bl_c("pcre2_regcomp");                                              // compile regex through PCRE2
    emitter.instruction("cbnz x0, __rt_mb_ereg_match_no");                      // compile failure produces no match

    // -- null-terminate subject --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // reload subject pointer for C-string materialization
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // reload subject length for C-string materialization
    emitter.instruction("bl __rt_cstr2");                                       // materialize subject C string in x0
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string

    // -- execute regex: pcre2_regexec(&regex_t, subject, 1, &regmatch_t, 0) --
    emitter.instruction("mov x0, sp");                                          // pass compiled regex_t storage to regexec
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_cstr_off));       // pass subject C string to regexec
    emitter.instruction("mov x2, #1");                                          // request one regmatch slot for rm_so
    emitter.instruction(&format!("add x3, sp, #{}", regmatch_off));             // pass local regmatch_t output buffer
    emitter.instruction("mov x4, #0");                                          // use default regexec flags
    emitter.bl_c("pcre2_regexec");                                              // execute regex through PCRE2
    emitter.instruction(&format!("str x0, [sp, #{}]", regexec_result_off));     // save regexec status across cleanup

    // -- free compiled regex --
    emitter.instruction("mov x0, sp");                                          // reload compiled regex_t storage for regfree
    emitter.bl_c("pcre2_regfree");                                              // release compiled regex resources

    // -- no match if regexec != 0 --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", regexec_result_off));     // reload regexec status after regfree
    emitter.instruction("cbnz x0, __rt_mb_ereg_match_no");                      // nonzero regexec status means no match

    // -- anchored-at-start: require regmatch[0].rm_so == 0 --
    emitter.instruction(&emitter.platform.regoff_load_instr("x9", "sp", regmatch_off)); // load rm_so with the native regoff_t width
    emitter.instruction("cbnz x9, __rt_mb_ereg_match_no");                      // reject matches that begin after offset zero
    emitter.instruction("mov x0, #1");                                          // return true for a start-anchored match
    emitter.instruction("b __rt_mb_ereg_match_ret");                            // share the common epilogue

    emitter.label("__rt_mb_ereg_match_no");
    emitter.instruction("mov x0, #0");                                          // return false for compile failures and non-matches

    emitter.label("__rt_mb_ereg_match_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // release the mb_ereg_match stack frame
    emitter.instruction("ret");                                                 // return the boolean result in x0
}

/// Emits the x86_64 Linux variant of `__rt_mb_ereg_match`.
fn emit_mb_ereg_match_linux_x86_64(emitter: &mut Emitter) {
    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let pattern_ptr_off = regmatch_off + emitter.platform.regmatch_t_size();
    let pattern_len_off = pattern_ptr_off + 8;
    let subject_ptr_off = pattern_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let options_ptr_off = subject_len_off + 8;
    let options_len_off = options_ptr_off + 8;
    let pattern_cstr_off = options_len_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let regexec_result_off = subject_cstr_off + 8;
    let stack_size = (regexec_result_off + 16 + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: mb_ereg_match (start-anchored regex) ---");
    emitter.label_global("__rt_mb_ereg_match");

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction(&format!("sub rsp, {}", stack_size));                   // reserve aligned local storage

    // -- save inputs (rdi/rsi = pattern, rdx/rcx = subject, r8/r9 = options) --
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdi", pattern_ptr_off)); // save pattern pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rsi", pattern_len_off)); // save pattern length
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", subject_ptr_off)); // save subject pointer
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", subject_len_off)); // save subject length
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r8", options_ptr_off)); // save options pointer, or zero for null options
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r9", options_len_off)); // save options length, or zero for null options

    // -- materialize the bare pattern as a null-terminated C string (__rt_cstr2: rax=ptr, rdx=len) --
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", pattern_ptr_off)); // reload pattern pointer for C-string materialization
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", pattern_len_off)); // reload pattern length for C-string materialization
    emitter.instruction("call __rt_cstr2");                                     // materialize pattern C string in rax
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off)); // save pattern C string

    super::emit_prepare_regex_locale(emitter);

    // -- translate supported mb_ereg_match options into PCRE2 POSIX compile flags --
    emitter.instruction("xor edx, edx");                                        // start with default PCRE2 POSIX compile flags
    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", options_len_off)); // load options length
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", options_ptr_off)); // load options pointer
    emitter.instruction("test r10, r10");                                       // are there any options bytes?
    emitter.instruction("jz __rt_mb_ereg_match_flags_ready_x86");               // no options bytes means default flags
    emitter.instruction("test r11, r11");                                       // is the options pointer null?
    emitter.instruction("jz __rt_mb_ereg_match_flags_ready_x86");               // null options pointer means default flags
    emitter.instruction("add r10, r11");                                        // compute one-past-end options pointer
    emitter.label("__rt_mb_ereg_match_flags_loop_x86");
    emitter.instruction("movzx r8d, BYTE PTR [r11]");                           // load one option byte
    emitter.instruction("add r11, 1");                                          // advance the options cursor
    emitter.instruction("cmp r8b, 105");                                        // is the option byte 'i'?
    emitter.instruction("jne __rt_mb_ereg_match_flags_skip_i_x86");             // ignore non-`i` option bytes for now
    emitter.instruction(&format!("or edx, {}", PCRE2_POSIX_REG_ICASE));         // add REG_ICASE for case-insensitive matching
    emitter.label("__rt_mb_ereg_match_flags_skip_i_x86");
    emitter.instruction("cmp r11, r10");                                        // have all option bytes been consumed?
    emitter.instruction("jl __rt_mb_ereg_match_flags_loop_x86");                // keep scanning while bytes remain
    emitter.label("__rt_mb_ereg_match_flags_ready_x86");

    // -- pcre2_regcomp(&regex_t, pattern, flags) --
    emitter.instruction("lea rdi, [rsp]");                                      // pass local regex_t storage as regcomp argument 1
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off)); // pass pattern C string as regcomp argument 2
    emitter.bl_c("pcre2_regcomp");                                              // compile regex through PCRE2
    emitter.instruction("test eax, eax");                                       // did regex compilation succeed?
    emitter.instruction("jnz __rt_mb_ereg_match_no_x86");                       // compile failure produces no match

    // -- null-terminate subject --
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off)); // reload subject pointer for C-string materialization
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off)); // reload subject length for C-string materialization
    emitter.instruction("call __rt_cstr2");                                     // materialize subject C string in rax
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off)); // save subject C string

    // -- pcre2_regexec(&regex_t, subject, 1, &regmatch_t, 0) --
    emitter.instruction("lea rdi, [rsp]");                                      // pass compiled regex_t storage to regexec
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", subject_cstr_off)); // pass subject C string to regexec
    emitter.instruction("mov edx, 1");                                          // request one regmatch slot for rm_so
    emitter.instruction(&format!("lea rcx, [rsp + {}]", regmatch_off));         // pass local regmatch_t output buffer
    emitter.instruction("xor r8d, r8d");                                        // use default regexec flags
    emitter.bl_c("pcre2_regexec");                                              // execute regex through PCRE2
    emitter.instruction(&format!("mov DWORD PTR [rsp + {}], eax", regexec_result_off)); // save regexec status across cleanup

    // -- free compiled regex --
    emitter.instruction("lea rdi, [rsp]");                                      // reload compiled regex_t storage for regfree
    emitter.bl_c("pcre2_regfree");                                              // release compiled regex resources

    // -- no match if regexec != 0 --
    emitter.instruction(&format!("mov eax, DWORD PTR [rsp + {}]", regexec_result_off)); // reload regexec status after regfree
    emitter.instruction("test eax, eax");                                       // interpret zero regexec status as a match
    emitter.instruction("jnz __rt_mb_ereg_match_no_x86");                       // nonzero regexec status means no match

    // -- anchored-at-start: require regmatch[0].rm_so == 0 (signed 32-bit regoff_t) --
    emitter.instruction(&format!("movsxd rax, DWORD PTR [rsp + {}]", regmatch_off)); // load rm_so with the native regoff_t width
    emitter.instruction("test rax, rax");                                       // did the match start at offset zero?
    emitter.instruction("jnz __rt_mb_ereg_match_no_x86");                       // reject matches that begin after offset zero
    emitter.instruction("mov eax, 1");                                          // return true for a start-anchored match
    emitter.instruction("jmp __rt_mb_ereg_match_ret_x86");                      // share the common epilogue

    emitter.label("__rt_mb_ereg_match_no_x86");
    emitter.instruction("xor eax, eax");                                        // return false for compile failures and non-matches

    emitter.label("__rt_mb_ereg_match_ret_x86");
    emitter.instruction(&format!("add rsp, {}", stack_size));                   // release the mb_ereg_match stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boolean result in eax
}
