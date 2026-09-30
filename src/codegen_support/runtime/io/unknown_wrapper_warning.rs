//! Purpose:
//! Emits `__rt_unknown_wrapper_warning`, the diagnostic PHP prints when a path names a
//! `scheme://` that no wrapper — built-in or registered — provides.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `__rt_fopen` and `__rt_file_get_contents` failure paths, immediately before
//!   `__rt_open_failed_warning`.
//!
//! Key details:
//! - PHP emits TWO warnings for `fopen("bogus://x", "r")`. The first names the scheme; the
//!   second is the ordinary failed-open line, which reports "No such file or directory" — true
//!   of the path, and silent about the cause. elephc emitted only the second.
//! - The check has to run at RUN TIME, not at lowering: `stream_wrapper_register()` is a runtime
//!   call, so a scheme the compiler has never heard of may be perfectly valid by the time the
//!   open happens. Both authorities are consulted — `__rt_path_is_wrapper` for the userspace
//!   table and `__rt_builtin_wrapper_index` for the built-ins — and the warning is emitted only
//!   when neither knows the scheme.
//! - A path with no `://` at all is an ordinary filesystem path and warns nothing.
//! - Neither is a ONE-LETTER scheme: php-src requires `p - path > 1` before it reads what
//!   precedes `://` as a scheme, so `c://x` is a Windows drive path on every platform and gets
//!   only the ordinary failed-open line. MEASURED on `php -n` 8.5.6.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Bytes reserved for the composed message. A caller name, a clamped scheme and the two fixed
/// fragments fit with room to spare.
pub(crate) const UNKNOWN_WRAPPER_MSG_CAPACITY: usize = 320;

/// The most scheme bytes copied into the message.
const SCHEME_CLAMP: usize = 64;

/// Emits `__rt_unknown_wrapper_warning(prefix_ptr, prefix_len, path_cstr)`.
///
/// AArch64 takes `x0`/`x1`/`x2`; x86_64 takes `rdi`/`rsi`/`rdx` — the same shape as
/// `__rt_open_failed_warning`, so the two calls sit together at a failure site with the same
/// operands. The path is NUL-terminated and its length is measured here. Emits nothing and
/// returns quietly when the path carries no scheme, or when some wrapper claims it.
pub fn emit_unknown_wrapper_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 composer.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: compose the unknown-wrapper warning ---");
    emitter.label_global("__rt_unknown_wrapper_warning");
    // Frame: [0] prefix ptr, [8] prefix len, [16] path ptr, [24] path len, [32] scheme len,
    //        [40] destination cursor, [48] linkage.
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #48]");
    emitter.instruction("add x29, sp, #48");
    emitter.instruction("stp x0, x1, [sp, #0]");                                // the caller's name
    emitter.instruction("str x2, [sp, #16]");                                   // the path being opened

    // -- measure the NUL-terminated path --
    emitter.instruction("mov x3, #0");
    emitter.label("__rt_uww_len");
    emitter.instruction("ldrb w11, [x2, x3]");
    emitter.instruction("cbz w11, __rt_uww_len_done");
    emitter.instruction("add x3, x3, #1");
    emitter.instruction("b __rt_uww_len");
    emitter.label("__rt_uww_len_done");
    emitter.instruction("str x3, [sp, #24]");                                   // the path length

    // -- find the "://" that separates the scheme, if there is one --
    emitter.instruction("mov x9, #0");                                          // scan index
    emitter.label("__rt_uww_scan");
    emitter.instruction("add x10, x9, #3");                                     // the separator needs three bytes
    emitter.instruction("cmp x10, x3");
    emitter.instruction("b.hi __rt_uww_ret");                                   // no room left: no scheme, no warning
    emitter.instruction("ldrb w11, [x2, x9]");
    emitter.instruction("cmp w11, #0x3a");                                      // ':'
    emitter.instruction("b.ne __rt_uww_scan_next");
    emitter.instruction("add x12, x2, x9");
    emitter.instruction("ldrb w11, [x12, #1]");
    emitter.instruction("cmp w11, #0x2f");                                      // '/'
    emitter.instruction("b.ne __rt_uww_scan_next");
    emitter.instruction("ldrb w11, [x12, #2]");
    emitter.instruction("cmp w11, #0x2f");                                      // '/'
    emitter.instruction("b.eq __rt_uww_found");
    emitter.label("__rt_uww_scan_next");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("b __rt_uww_scan");

    emitter.label("__rt_uww_found");
    emitter.instruction("cmp x9, #2");                                          // php-src: `p - path > 1`
    emitter.instruction("b.lo __rt_uww_ret");                                   // one letter is a DRIVE, not a wrapper name
    emitter.instruction("str x9, [sp, #32]");                                   // the scheme length

    // -- ask both authorities; either one claiming the scheme means there is nothing to report --
    emitter.instruction("ldr x0, [sp, #16]");                                   // path pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // path length
    emitter.instruction("bl __rt_path_is_wrapper");                             // registered userspace wrapper?
    emitter.instruction("cbnz x0, __rt_uww_ret");                               // yes: the open failed for another reason
    emitter.instruction("ldr x0, [sp, #16]");                                   // scheme pointer (the path starts with it)
    emitter.instruction("ldr x1, [sp, #32]");                                   // scheme length
    emitter.instruction("bl __rt_builtin_wrapper_index");                       // built-in wrapper?
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.ge __rt_uww_ret");                                   // yes: likewise

    // -- compose "Warning: <prefix>(): Unable to find the wrapper "<scheme>" - ..." --
    abi::emit_symbol_address(emitter, "x9", "_unknown_wrapper_msg");
    emitter.instruction("mov x10, #0");                                         // bytes written so far
    abi::emit_symbol_address(emitter, "x13", "_unknown_wrapper_head");
    emitter.instruction(&format!(
        "mov x14, #{}",
        super::super::data::UNKNOWN_WRAPPER_HEAD.len()
    ));
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_uww_head_loop");
    emitter.instruction("cmp x11, x14");
    emitter.instruction("b.hs __rt_uww_prefix");
    emitter.instruction("ldrb w12, [x13, x11]");
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_uww_head_loop");

    emitter.label("__rt_uww_prefix");
    emitter.instruction("ldr x13, [sp, #0]");                                   // the caller's name
    emitter.instruction("ldr x14, [sp, #8]");
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_uww_prefix_loop");
    emitter.instruction("cmp x11, x14");
    emitter.instruction("b.hs __rt_uww_mid");
    emitter.instruction("ldrb w12, [x13, x11]");
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_uww_prefix_loop");

    emitter.label("__rt_uww_mid");
    abi::emit_symbol_address(emitter, "x13", "_unknown_wrapper_mid");
    emitter.instruction(&format!(
        "mov x14, #{}",
        super::super::data::UNKNOWN_WRAPPER_MIDDLE.len()
    ));
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_uww_mid_loop");
    emitter.instruction("cmp x11, x14");
    emitter.instruction("b.hs __rt_uww_scheme");
    emitter.instruction("ldrb w12, [x13, x11]");
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_uww_mid_loop");

    emitter.label("__rt_uww_scheme");
    emitter.instruction("ldr x13, [sp, #16]");                                  // the scheme sits at the path start
    emitter.instruction("ldr x14, [sp, #32]");
    emitter.instruction(&format!("mov x15, #{SCHEME_CLAMP}"));
    emitter.instruction("cmp x14, x15");
    emitter.instruction("csel x14, x14, x15, ls");                              // never write past the buffer
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_uww_scheme_loop");
    emitter.instruction("cmp x11, x14");
    emitter.instruction("b.hs __rt_uww_tail");
    emitter.instruction("ldrb w12, [x13, x11]");
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_uww_scheme_loop");

    emitter.label("__rt_uww_tail");
    abi::emit_symbol_address(emitter, "x13", "_unknown_wrapper_tail");
    emitter.instruction(&format!(
        "mov x14, #{}",
        super::super::data::UNKNOWN_WRAPPER_TAIL.len()
    ));
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_uww_tail_loop");
    emitter.instruction("cmp x11, x14");
    emitter.instruction("b.hs __rt_uww_emit");
    emitter.instruction("ldrb w12, [x13, x11]");
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_uww_tail_loop");

    emitter.label("__rt_uww_emit");
    emitter.instruction("mov x1, x9");                                          // message pointer
    emitter.instruction("mov x2, x10");                                         // message length
    emitter.instruction("bl __rt_diag_warning");                                // stderr, and `@` suppresses it

    emitter.label("__rt_uww_ret");
    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}

/// Emits the Linux x86_64 composer.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: compose the unknown-wrapper warning ---");
    emitter.label_global("__rt_unknown_wrapper_warning");
    // Frame: [rbp-8] prefix ptr, [rbp-16] prefix len, [rbp-24] path ptr, [rbp-32] path len,
    //        [rbp-40] scheme len, [rbp-48] destination cursor, [rbp-56] buffer base.
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 64");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the caller's name
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // the path being opened

    // -- measure the NUL-terminated path --
    emitter.instruction("xor rcx, rcx");
    emitter.label("__rt_uww_x_len");
    emitter.instruction("movzx r11d, BYTE PTR [rdx + rcx]");
    emitter.instruction("test r11b, r11b");
    emitter.instruction("jz __rt_uww_x_len_done");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_uww_x_len");
    emitter.label("__rt_uww_x_len_done");
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // the path length

    // -- find the "://" that separates the scheme, if there is one --
    emitter.instruction("xor r9, r9");                                          // scan index
    emitter.label("__rt_uww_x_scan");
    emitter.instruction("mov r10, r9");
    emitter.instruction("add r10, 3");                                          // the separator needs three bytes
    emitter.instruction("cmp r10, rcx");
    emitter.instruction("ja __rt_uww_x_ret");                                   // no room left: no scheme, no warning
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r9]");
    emitter.instruction("cmp r11b, 0x3a");                                      // ':'
    emitter.instruction("jne __rt_uww_x_scan_next");
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r9 + 1]");
    emitter.instruction("cmp r11b, 0x2f");                                      // '/'
    emitter.instruction("jne __rt_uww_x_scan_next");
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r9 + 2]");
    emitter.instruction("cmp r11b, 0x2f");                                      // '/'
    emitter.instruction("je __rt_uww_x_found");
    emitter.label("__rt_uww_x_scan_next");
    emitter.instruction("inc r9");
    emitter.instruction("jmp __rt_uww_x_scan");

    emitter.label("__rt_uww_x_found");
    emitter.instruction("cmp r9, 2");                                           // php-src: `p - path > 1`
    emitter.instruction("jb __rt_uww_x_ret");                                   // one letter is a DRIVE, not a wrapper name
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // the scheme length

    // -- ask both authorities; either one claiming the scheme means there is nothing to report --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // path pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // path length
    emitter.instruction("call __rt_path_is_wrapper");                           // registered userspace wrapper?
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_uww_x_ret");                                  // yes: the open failed for another reason
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // scheme pointer (the path starts with it)
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // scheme length
    emitter.instruction("call __rt_builtin_wrapper_index");                     // built-in wrapper?
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jge __rt_uww_x_ret");                                  // yes: likewise

    // -- compose "Warning: <prefix>(): Unable to find the wrapper "<scheme>" - ..." --
    abi::emit_symbol_address(emitter, "r8", "_unknown_wrapper_msg");
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // the buffer base
    emitter.instruction("xor r10, r10");                                        // bytes written so far
    abi::emit_symbol_address(emitter, "rsi", "_unknown_wrapper_head");
    emitter.instruction(&format!(
        "mov rcx, {}",
        super::super::data::UNKNOWN_WRAPPER_HEAD.len()
    ));
    emitter.instruction("xor r9, r9");
    emitter.label("__rt_uww_x_head_loop");
    emitter.instruction("cmp r9, rcx");
    emitter.instruction("jae __rt_uww_x_prefix");
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r9]");
    emitter.instruction("mov BYTE PTR [r8 + r10], r11b");
    emitter.instruction("inc r10");
    emitter.instruction("inc r9");
    emitter.instruction("jmp __rt_uww_x_head_loop");

    emitter.label("__rt_uww_x_prefix");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // the caller's name
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");
    emitter.instruction("xor r9, r9");
    emitter.label("__rt_uww_x_prefix_loop");
    emitter.instruction("cmp r9, rcx");
    emitter.instruction("jae __rt_uww_x_mid");
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r9]");
    emitter.instruction("mov BYTE PTR [r8 + r10], r11b");
    emitter.instruction("inc r10");
    emitter.instruction("inc r9");
    emitter.instruction("jmp __rt_uww_x_prefix_loop");

    emitter.label("__rt_uww_x_mid");
    abi::emit_symbol_address(emitter, "rsi", "_unknown_wrapper_mid");
    emitter.instruction(&format!(
        "mov rcx, {}",
        super::super::data::UNKNOWN_WRAPPER_MIDDLE.len()
    ));
    emitter.instruction("xor r9, r9");
    emitter.label("__rt_uww_x_mid_loop");
    emitter.instruction("cmp r9, rcx");
    emitter.instruction("jae __rt_uww_x_scheme");
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r9]");
    emitter.instruction("mov BYTE PTR [r8 + r10], r11b");
    emitter.instruction("inc r10");
    emitter.instruction("inc r9");
    emitter.instruction("jmp __rt_uww_x_mid_loop");

    emitter.label("__rt_uww_x_scheme");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // the scheme sits at the path start
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");
    // The counter is cleared BEFORE the clamp branch, not after it. Clearing it only on the
    // clamped path left the common case entering the loop with the previous fragment's counter
    // still in r9, which is already past the length — so the loop exited at once and the scheme
    // came out empty. The AArch64 side clamps with `csel` and cannot express the same slip.
    emitter.instruction("xor r9, r9");
    emitter.instruction(&format!("cmp rcx, {SCHEME_CLAMP}"));
    emitter.instruction("jbe __rt_uww_x_scheme_loop");
    emitter.instruction(&format!("mov rcx, {SCHEME_CLAMP}"));                   // never write past the buffer
    emitter.label("__rt_uww_x_scheme_loop");
    emitter.instruction("cmp r9, rcx");
    emitter.instruction("jae __rt_uww_x_tail");
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r9]");
    emitter.instruction("mov BYTE PTR [r8 + r10], r11b");
    emitter.instruction("inc r10");
    emitter.instruction("inc r9");
    emitter.instruction("jmp __rt_uww_x_scheme_loop");

    emitter.label("__rt_uww_x_tail");
    abi::emit_symbol_address(emitter, "rsi", "_unknown_wrapper_tail");
    emitter.instruction(&format!(
        "mov rcx, {}",
        super::super::data::UNKNOWN_WRAPPER_TAIL.len()
    ));
    emitter.instruction("xor r9, r9");
    emitter.label("__rt_uww_x_tail_loop");
    emitter.instruction("cmp r9, rcx");
    emitter.instruction("jae __rt_uww_x_emit");
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r9]");
    emitter.instruction("mov BYTE PTR [r8 + r10], r11b");
    emitter.instruction("inc r10");
    emitter.instruction("inc r9");
    emitter.instruction("jmp __rt_uww_x_tail_loop");

    emitter.label("__rt_uww_x_emit");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // message pointer
    emitter.instruction("mov rsi, r10");                                        // message length
    emitter.instruction("call __rt_diag_warning");                              // stderr, and `@` suppresses it

    emitter.label("__rt_uww_x_ret");
    emitter.instruction("leave");
    emitter.instruction("ret");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Every copy loop must clear its counter on the path that REACHES it, not only on the
    /// clamped one.
    ///
    /// The x86_64 scheme copy cleared `r9` after the clamp branch, so the ordinary case — a
    /// scheme shorter than the clamp — jumped straight into the loop carrying the previous
    /// fragment's counter, which is already past the length. The loop exited at once and the
    /// warning read `Unable to find the wrapper ""`. AArch64 clamps with `csel` and cannot
    /// express the same slip, so this is pinned on the emitted assembly instead of a run.
    #[test]
    fn test_x86_64_clears_the_scheme_counter_before_the_clamp_branch() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_unknown_wrapper_warning(&mut emitter);
        let asm = emitter.output();
        // The counter is cleared in every copy loop, so the check is scoped to the span between
        // the scheme block's label and its clamp — a whole-file search would happily match some
        // other loop's clear and pass while this one was still missing.
        let block = asm
            .find("__rt_uww_x_scheme:")
            .expect("the scheme copy block must be labelled");
        let clamp = asm[block..]
            .find(&format!("cmp rcx, {SCHEME_CLAMP}"))
            .map(|at| block + at)
            .expect("the scheme copy must clamp its length");
        assert!(
            asm[block..clamp].contains("xor r9, r9"),
            "the counter must be cleared BEFORE the clamp branch, or the unclamped path skips it"
        );
    }

    /// Both architectures must consult BOTH wrapper authorities before warning.
    ///
    /// `stream_wrapper_register()` is a runtime call, so a scheme the compiler never heard of
    /// can be valid by the time the open happens; and a built-in scheme must never be reported
    /// missing. Emitting the warning without either lookup would be a false positive on
    /// perfectly ordinary code.
    #[test]
    fn test_both_wrapper_authorities_are_consulted_before_warning() {
        for arch in [Arch::AArch64, Arch::X86_64] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_unknown_wrapper_warning(&mut emitter);
            let asm = emitter.output();
            for helper in ["__rt_path_is_wrapper", "__rt_builtin_wrapper_index"] {
                assert!(
                    asm.contains(helper),
                    "{arch:?} must ask {helper} before reporting a missing wrapper"
                );
            }
            let warn = asm
                .find("__rt_diag_warning")
                .expect("the helper must be able to warn");
            let last_lookup = asm
                .rfind("__rt_builtin_wrapper_index")
                .expect("the built-in lookup must be emitted");
            assert!(
                last_lookup < warn,
                "{arch:?} must consult the built-ins BEFORE composing the warning"
            );
        }
    }
}
