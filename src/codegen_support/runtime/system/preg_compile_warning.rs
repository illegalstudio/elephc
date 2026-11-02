//! Purpose:
//! Emits `__rt_preg_compile_warning`, which reports php's `Compilation failed` warning for the
//! pattern the last `preg_*` runtime helper could not compile.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//! - Emitted code, right after every `preg_*` runtime helper call (`lower_inst::builtins::regex`).
//!
//! Key details:
//! - The helpers treat a pattern PCRE2 rejects as "no match"; php also warns
//!   `<function>(): Compilation failed: <reason> at offset <n>`. The shim records the last compile
//!   failure and builds that line for the function name the call site passes, so no helper needs
//!   to know which PHP function it serves.
//! - Called after helpers whose results live in argument registers (a match count, a string
//!   pair, a `$matches` pointer, a replacement count), so every one of them is preserved.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the compile-failure warning helper for the current target.
///
/// # ABI
/// - ARM64: `x9` = PHP function name pointer, `x10` = its length; `x0`-`x7` preserved; out `x9`
///   = 1 when the pattern failed to compile (and was reported), 0 otherwise.
/// - x86_64: `r10` = PHP function name pointer, `r11` = its length; `rax`, `rcx`, `rdx`, `rsi`,
///   `rdi`, `r8` and `r9` preserved; out `r10` = the same failure flag.
pub fn emit_preg_compile_warning(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_preg_compile_warning_x86_64(emitter);
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: preg_compile_warning ---");
    emitter.label_global("__rt_preg_compile_warning");
    emitter.instruction("sub sp, sp, #112");                                    // frame for x0-x7, the message pair, the flag and fp/lr
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the helper frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // preserve the preg helper's result registers
    emitter.instruction("stp x2, x3, [sp, #16]");
    emitter.instruction("stp x4, x5, [sp, #32]");
    emitter.instruction("stp x6, x7, [sp, #48]");
    emitter.instruction("mov x0, x9");                                          // PHP function name pointer
    emitter.instruction("mov x1, x10");                                         // PHP function name length
    emitter.instruction("add x2, sp, #64");                                     // receive the warning line pointer
    emitter.instruction("add x3, sp, #72");                                     // receive the warning line length
    emitter.bl_c("elephc_pcre2_v1_compile_warning");
    emitter.instruction("str x0, [sp, #80]");                                   // remember whether the compile failed
    emitter.instruction("cbz w0, __rt_preg_compile_warning_done");              // the last compile succeeded: nothing to report
    emitter.instruction("ldr x1, [sp, #64]");                                   // warning line pointer
    emitter.instruction("ldr x2, [sp, #72]");                                   // warning line length
    emitter.instruction("bl __rt_diag_warning");                                // emit it, or drop it under @
    emitter.label("__rt_preg_compile_warning_done");
    emitter.instruction("ldp x0, x1, [sp, #0]");                                // restore the preg helper's result registers
    emitter.instruction("ldp x2, x3, [sp, #16]");
    emitter.instruction("ldp x4, x5, [sp, #32]");
    emitter.instruction("ldp x6, x7, [sp, #48]");
    emitter.instruction("ldr w9, [sp, #80]");                                   // x9 = 1 when the pattern failed to compile
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the helper frame
    emitter.instruction("ret");
}

/// Emits the Linux x86_64 compile-failure warning helper.
fn emit_preg_compile_warning_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_compile_warning ---");
    emitter.label_global("__rt_preg_compile_warning");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned helper frame
    emitter.instruction("sub rsp, 80");                                         // slots for seven registers and the message pair
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the preg helper's result registers
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");
    emitter.instruction("mov rdi, r10");                                        // PHP function name pointer
    emitter.instruction("mov rsi, r11");                                        // PHP function name length
    emitter.instruction("lea rdx, [rbp - 64]");                                 // receive the warning line pointer
    emitter.instruction("lea rcx, [rbp - 72]");                                 // receive the warning line length
    emitter.bl_c("elephc_pcre2_v1_compile_warning");
    emitter.instruction("mov DWORD PTR [rbp - 80], eax");                       // remember whether the compile failed
    emitter.instruction("test eax, eax");                                       // did the last compile fail?
    emitter.instruction("jz __rt_preg_compile_warning_done");                   // no: nothing to report
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // warning line pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // warning line length
    emitter.instruction("call __rt_diag_warning");                              // emit it, or drop it under @
    emitter.label("__rt_preg_compile_warning_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the preg helper's result registers
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");
    emitter.instruction("mov r10d, DWORD PTR [rbp - 80]");                      // r10 = 1 when the pattern failed to compile
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
