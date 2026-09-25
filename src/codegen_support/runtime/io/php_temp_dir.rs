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
    emitter.instruction("sub sp, sp, #16");                                     // frame for the nested __rt_getenv call
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    abi::emit_symbol_address(emitter, "x1", "_tmpdir_env_name");                // name pointer for the environment lookup
    emitter.instruction("mov x2, #6");                                          // "TMPDIR" is six bytes
    emitter.instruction("bl __rt_getenv");                                      // x1 = value pointer, x2 = value length
    emitter.instruction("cbz x2, __rt_php_temp_dir_fallback");                  // unset or empty TMPDIR falls back to P_tmpdir

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
    abi::emit_call_label(emitter, "__rt_getenv");                               // rax = value pointer, rdx = value length
    emitter.instruction("test rdx, rdx");                                       // did TMPDIR hold anything?
    emitter.instruction("jz __rt_php_temp_dir_fallback_x86");                   // unset or empty TMPDIR falls back to P_tmpdir

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
