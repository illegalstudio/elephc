//! Purpose:
//! Emits `__rt_mb_ereg_match`, the runtime helper for PHP's `mb_ereg_match()`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//! - The `mb_ereg_match()` builtin lowering
//!   (`crate::codegen_ir::lower_inst::builtins::regex::lower_mb_ereg_match`).
//!
//! Key details:
//! - `mb_ereg_match($pattern, $string)` is an ANCHORED-AT-START match: it returns true iff the
//!   pattern matches beginning at offset 0 of the subject (verified against PHP 8.5:
//!   `mb_ereg_match('bc','abc')` is false, `mb_ereg_match('ab','abc')` is true). It does NOT
//!   require matching to the end unless the pattern says so (`\z`/`$`).
//! - Unlike `preg_match`, the pattern carries NO delimiters and is a bare POSIX/Perl regex, so we
//!   materialize it verbatim as a C string (no `__rt_preg_strip` / `__rt_pcre_to_posix`). PCRE2's
//!   POSIX wrapper always compiles with Perl semantics, so `\z` and friends work with flags = 0.
//! - Start-anchoring is enforced by checking `regmatch[0].rm_so == 0` after a successful
//!   `pcre2_regexec` (PCRE2 returns the leftmost match, so `rm_so == 0` iff it matches at the start).
//! - Input: x1=pattern ptr, x2=pattern len, x3=subject ptr, x4=subject len (AArch64) /
//!   rdi/rsi (pattern) + rdx/rcx (subject) (x86_64). Output: 1 if matched at start, else 0.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits `__rt_mb_ereg_match(pattern, subject) -> bool`, the start-anchored regex matcher.
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
    let pattern_cstr_off = subject_len_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let regexec_result_off = subject_cstr_off + 8;
    let stack_size = (regexec_result_off + 40 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: mb_ereg_match (start-anchored regex) ---");
    emitter.label_global("__rt_mb_ereg_match");

    emitter.instruction(&format!("sub sp, sp, #{}", stack_size)); // allocate mb_ereg_match stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off)); // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off)); // set new frame pointer

    // -- save inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off)); // save pattern ptr
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off)); // save pattern len
    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off)); // save subject ptr
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off)); // save subject len

    // -- materialize the bare pattern as a null-terminated C string (no delimiters to strip) --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_ptr_off)); // pattern ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", pattern_len_off)); // pattern len
    emitter.instruction("bl __rt_cstr2"); // → x0 = pattern C string
    emitter.instruction(&format!("str x0, [sp, #{}]", pattern_cstr_off)); // save pattern C string

    super::emit_prepare_regex_locale(emitter);

    // -- compile regex: pcre2_regcomp(&regex_t, pattern, 0) -- (0 = default; PCRE2 posix uses Perl syntax)
    emitter.instruction("mov x0, sp"); // x0 = regex_t at sp+0
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_cstr_off)); // x1 = pattern C string
    emitter.instruction("mov x2, #0"); // flags = 0
    emitter.bl_c("pcre2_regcomp"); // compile regex through PCRE2
    emitter.instruction("cbnz x0, __rt_mb_ereg_match_no"); // compile failed → no match

    // -- null-terminate subject --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off)); // subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off)); // subject len
    emitter.instruction("bl __rt_cstr2"); // → x0 = subject C string
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off)); // save subject C string

    // -- execute regex: pcre2_regexec(&regex_t, subject, 1, &regmatch_t, 0) --
    emitter.instruction("mov x0, sp"); // x0 = regex_t
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_cstr_off)); // x1 = subject C string
    emitter.instruction("mov x2, #1"); // nmatch = 1 (need regmatch[0] for rm_so)
    emitter.instruction(&format!("add x3, sp, #{}", regmatch_off)); // x3 = regmatch_t buffer
    emitter.instruction("mov x4, #0"); // eflags = 0
    emitter.bl_c("pcre2_regexec"); // regexec → x0=0 if match
    emitter.instruction(&format!("str x0, [sp, #{}]", regexec_result_off)); // save regexec result

    // -- free compiled regex --
    emitter.instruction("mov x0, sp");
    emitter.bl_c("pcre2_regfree");

    // -- no match if regexec != 0 --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", regexec_result_off));
    emitter.instruction("cbnz x0, __rt_mb_ereg_match_no");
    // -- anchored-at-start: require regmatch[0].rm_so == 0 --
    emitter.instruction(&emitter.platform.regoff_load_instr("x9", "sp", regmatch_off)); // x9 = rm_so
    emitter.instruction("cbnz x9, __rt_mb_ereg_match_no"); // match did not begin at offset 0
    emitter.instruction("mov x0, #1"); // matched at start → true
    emitter.instruction("b __rt_mb_ereg_match_ret");

    emitter.label("__rt_mb_ereg_match_no");
    emitter.instruction("mov x0, #0"); // no match → false

    emitter.label("__rt_mb_ereg_match_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off)); // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size)); // deallocate stack frame
    emitter.instruction("ret");
}

/// Emits the x86_64 Linux variant of `__rt_mb_ereg_match`.
fn emit_mb_ereg_match_linux_x86_64(emitter: &mut Emitter) {
    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let pattern_ptr_off = regmatch_off + emitter.platform.regmatch_t_size();
    let pattern_len_off = pattern_ptr_off + 8;
    let subject_ptr_off = pattern_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let pattern_cstr_off = subject_len_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let regexec_result_off = subject_cstr_off + 8;
    let stack_size = (regexec_result_off + 16 + 15) & !15;

    emitter.blank();
    emitter.comment("--- runtime: mb_ereg_match (start-anchored regex) ---");
    emitter.label_global("__rt_mb_ereg_match");

    emitter.instruction("push rbp"); // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp"); // stable frame base
    emitter.instruction(&format!("sub rsp, {}", stack_size)); // reserve local storage

    // -- save inputs (rdi/rsi = pattern ptr/len, rdx/rcx = subject ptr/len) --
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdi", pattern_ptr_off));
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rsi", pattern_len_off));
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", subject_ptr_off));
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rcx", subject_len_off));

    // -- materialize the bare pattern as a null-terminated C string (__rt_cstr2: rax=ptr, rdx=len) --
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", pattern_ptr_off));
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", pattern_len_off));
    emitter.instruction("call __rt_cstr2");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", pattern_cstr_off));

    super::emit_prepare_regex_locale(emitter);

    // -- pcre2_regcomp(&regex_t, pattern, 0) --
    emitter.instruction("lea rdi, [rsp]");
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", pattern_cstr_off));
    emitter.instruction("xor edx, edx"); // flags = 0
    emitter.bl_c("pcre2_regcomp");
    emitter.instruction("test eax, eax");
    emitter.instruction("jnz __rt_mb_ereg_match_no_x86");

    // -- null-terminate subject --
    emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {}]", subject_ptr_off));
    emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", subject_len_off));
    emitter.instruction("call __rt_cstr2");
    emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", subject_cstr_off));

    // -- pcre2_regexec(&regex_t, subject, 1, &regmatch_t, 0) --
    emitter.instruction("lea rdi, [rsp]");
    emitter.instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", subject_cstr_off));
    emitter.instruction("mov edx, 1");
    emitter.instruction(&format!("lea rcx, [rsp + {}]", regmatch_off));
    emitter.instruction("xor r8d, r8d");
    emitter.bl_c("pcre2_regexec");
    emitter.instruction(&format!("mov DWORD PTR [rsp + {}], eax", regexec_result_off));

    // -- free compiled regex --
    emitter.instruction("lea rdi, [rsp]");
    emitter.bl_c("pcre2_regfree");

    // -- no match if regexec != 0 --
    emitter.instruction(&format!("mov eax, DWORD PTR [rsp + {}]", regexec_result_off));
    emitter.instruction("test eax, eax");
    emitter.instruction("jnz __rt_mb_ereg_match_no_x86");
    // -- anchored-at-start: require regmatch[0].rm_so == 0 (signed 32-bit regoff_t) --
    emitter.instruction(&format!("movsxd rax, DWORD PTR [rsp + {}]", regmatch_off));
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_mb_ereg_match_no_x86");
    emitter.instruction("mov eax, 1"); // matched at start → true
    emitter.instruction("jmp __rt_mb_ereg_match_ret_x86");

    emitter.label("__rt_mb_ereg_match_no_x86");
    emitter.instruction("xor eax, eax"); // no match → false

    emitter.label("__rt_mb_ereg_match_ret_x86");
    emitter.instruction(&format!("add rsp, {}", stack_size));
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}
