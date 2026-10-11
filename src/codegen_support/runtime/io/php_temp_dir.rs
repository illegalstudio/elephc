//! Purpose:
//! Emits the `__rt_php_temp_dir` runtime helper, which resolves php's temporary
//! directory the way `php_get_temporary_directory` does.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - TMPDIR wins when it is set and non-empty, minus **exactly one** trailing slash,
//!   so `/var/tmp/probe///` resolves to `/var/tmp/probe//` and a bare `/` resolves to
//!   the empty string. Only an unset or empty TMPDIR falls back to `P_tmpdir`, which
//!   php returns verbatim — trailing slash included on macOS.
//! - Both `sys_get_temp_dir()` and `tmpfile()` read the directory through here, so
//!   the two can never disagree about where temporary files belong.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_php_temp_dir` for the current target.
///
/// Takes no arguments. Returns php's temporary directory as a PHP string in
/// `abi::string_result_regs` (`x1`/`x2` on AArch64, `rax`/`rdx` on x86_64). The
/// pointer is borrowed — it addresses either the process environment block or a
/// literal in the data section — so callers that mutate the bytes must copy first.
pub fn emit_php_temp_dir(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: php temporary directory ---");
    emitter.label_global("__rt_php_temp_dir");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 body of `__rt_php_temp_dir`.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #16");                                     // frame for the nested C-string and libc calls
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    abi::emit_symbol_address(emitter, "x1", "_tmpdir_env_name");                // name pointer for the environment lookup
    emitter.instruction("mov x2, #6");                                          // "TMPDIR" is six bytes
    emitter.instruction("bl __rt_cstr");                                        // terminate the static name for libc getenv
    emitter.emit_call_c("getenv");                                              // borrow TMPDIR directly from the process environment
    emitter.instruction("cbz x0, __rt_php_temp_dir_fallback");                  // an unset TMPDIR falls back to P_tmpdir
    emitter.instruction("mov x1, x0");                                          // return the borrowed environment pointer
    emitter.instruction("mov x2, #0");                                          // seed the environment-value length counter
    emitter.label("__rt_php_temp_dir_env_measure");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // inspect the next borrowed TMPDIR byte
    emitter.instruction("cbz w9, __rt_php_temp_dir_env_ready");                 // stop at the environment string terminator
    emitter.instruction("add x2, x2, #1");                                      // count this TMPDIR byte
    emitter.instruction("b __rt_php_temp_dir_env_measure");                     // continue measuring the environment value
    emitter.label("__rt_php_temp_dir_env_ready");
    emitter.instruction("cbz x2, __rt_php_temp_dir_fallback");                  // an empty TMPDIR falls back to P_tmpdir

    // -- drop exactly one trailing slash, as php does --
    emitter.instruction("sub x9, x2, #1");                                      // index of the last byte
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the last byte
    emitter.instruction("cmp w10, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_php_temp_dir_done");                         // nothing to strip
    emitter.instruction("mov x2, x9");                                          // shorten the string by that one slash
    emitter.instruction("b __rt_php_temp_dir_done");                            // TMPDIR wins over the fallback

    // -- P_tmpdir fallback: measure the literal, which php returns verbatim --
    emitter.label("__rt_php_temp_dir_fallback");
    abi::emit_symbol_address(emitter, "x1", "_php_p_tmpdir");                   // platform P_tmpdir literal
    emitter.instruction("mov x2, #0");                                          // seed the length counter
    emitter.label("__rt_php_temp_dir_measure");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load the next literal byte
    emitter.instruction("cbz w9, __rt_php_temp_dir_done");                      // stop at the C terminator
    emitter.instruction("add x2, x2, #1");                                      // count this byte
    emitter.instruction("b __rt_php_temp_dir_measure");                         // keep measuring

    emitter.label("__rt_php_temp_dir_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return ptr/len in the string result regs
}

/// Emits the x86_64 body of `__rt_php_temp_dir`.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    abi::emit_symbol_address(emitter, "rax", "_tmpdir_env_name");               // name pointer for the environment lookup
    emitter.instruction("mov rdx, 6");                                          // "TMPDIR" is six bytes
    abi::emit_call_label(emitter, "__rt_cstr");                                 // terminate the static name for libc getenv
    abi::emit_reg_move(emitter, abi::runtime_helper_int_arg_reg(emitter, 0), "rax");
    emitter.emit_call_c("getenv");                                              // borrow TMPDIR directly from the process environment
    emitter.instruction("test rax, rax");                                       // was TMPDIR set at all?
    emitter.instruction("jz __rt_php_temp_dir_fallback_x86");                   // an unset TMPDIR falls back to P_tmpdir
    emitter.instruction("mov rdx, 0");                                          // seed the environment-value length counter
    emitter.label("__rt_php_temp_dir_env_measure_x86");
    emitter.instruction("cmp BYTE PTR [rax + rdx], 0");                         // inspect the next borrowed TMPDIR byte
    emitter.instruction("je __rt_php_temp_dir_env_ready_x86");                  // stop at the environment string terminator
    emitter.instruction("add rdx, 1");                                          // count this TMPDIR byte
    emitter.instruction("jmp __rt_php_temp_dir_env_measure_x86");               // continue measuring the environment value
    emitter.label("__rt_php_temp_dir_env_ready_x86");
    emitter.instruction("test rdx, rdx");                                       // did TMPDIR contain any bytes?
    emitter.instruction("jz __rt_php_temp_dir_fallback_x86");                   // an empty TMPDIR falls back to P_tmpdir

    // -- drop exactly one trailing slash, as php does --
    emitter.instruction("mov r9, rdx");                                         // copy the length to index the last byte
    emitter.instruction("sub r9, 1");                                           // index of the last byte
    emitter.instruction("cmp BYTE PTR [rax + r9], 47");                         // is it '/'?
    emitter.instruction("jne __rt_php_temp_dir_done_x86");                      // nothing to strip
    emitter.instruction("mov rdx, r9");                                         // shorten the string by that one slash
    emitter.instruction("jmp __rt_php_temp_dir_done_x86");                      // TMPDIR wins over the fallback

    // -- P_tmpdir fallback: measure the literal, which php returns verbatim --
    emitter.label("__rt_php_temp_dir_fallback_x86");
    abi::emit_symbol_address(emitter, "rax", "_php_p_tmpdir");                  // platform P_tmpdir literal
    emitter.instruction("mov rdx, 0");                                          // seed the length counter
    emitter.label("__rt_php_temp_dir_measure_x86");
    emitter.instruction("cmp BYTE PTR [rax + rdx], 0");                         // reached the C terminator?
    emitter.instruction("je __rt_php_temp_dir_done_x86");                       // the literal is fully measured
    emitter.instruction("add rdx, 1");                                          // count this byte
    emitter.instruction("jmp __rt_php_temp_dir_measure_x86");                   // keep measuring

    emitter.label("__rt_php_temp_dir_done_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return ptr/len in the string result regs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Ensures the borrowed temp-directory helper never allocates an owned getenv copy.
    #[test]
    fn temp_dir_borrows_the_process_environment_on_both_architectures() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
            Target::new(Platform::Windows, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_php_temp_dir(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains(&target.extern_symbol("getenv")), "{target:?}");
            assert!(asm.contains("__rt_cstr"), "{target:?}");
            assert!(!asm.contains("__rt_getenv"), "{target:?}");
            assert!(!asm.contains("__rt_str_persist"), "{target:?}");
            if target.arch == Arch::X86_64 {
                assert!(asm.contains("mov rdi, rax"), "{target:?}");
            }
        }
    }
}
