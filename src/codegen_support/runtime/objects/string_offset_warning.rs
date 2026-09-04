//! Purpose:
//! Composes php-src's `Uninitialized string offset N` warning for an out-of-range `$s[$i]`.
//!
//! Called from:
//! - `__rt_mixed_array_get`, when a boxed string is indexed past either end.
//!
//! Key details:
//! - The offset is rendered in decimal INTO the message. Nothing else in the runtime renders
//!   an integer into a diagnostic — `sprintf`'s converter is private to `sprintf` — so the
//!   digits are produced here, backwards into a small stack window and then copied forward.
//! - php reports the offset the CALLER wrote, not the resolved one: `$s[-9]` says `-9`.
//! - Registers are all caller-saved by construction: x19-x28 / r12-r15 belong to the caller,
//!   and x16/x17 are IP0/IP1, which the linker may take for a long-branch veneer.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Bytes reserved for the composed message: the fixed text, a signed 64-bit decimal, and the
/// trailing newline, rounded up.
pub const STRING_OFFSET_MSG_CAPACITY: usize = 96;

/// The fixed text, which is also the number of bytes copied before the digits.
pub const STRING_OFFSET_PREFIX: &str = "Warning: Uninitialized string offset ";

/// Emits `__rt_warn_uninitialized_string_offset(offset)`.
pub fn emit_string_offset_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 composer.
///
/// x3 = value, x9 = message buffer, x4 = prefix, x10 = bytes written, x5/x6 = digit window
/// end and cursor, x12 = scratch byte. None of these is callee-saved or an IP register.
fn emit_aarch64(emitter: &mut Emitter) {
    let prefix_len = STRING_OFFSET_PREFIX.len();
    emitter.blank();
    emitter.comment("--- runtime: compose the uninitialized-string-offset warning ---");
    emitter.label_global("__rt_warn_uninitialized_string_offset");
    // Frame: [0..24] the backwards digit window, [32] linkage.
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");

    emitter.instruction("mov x3, x0");                                          // hold the caller's offset

    abi::emit_symbol_address(emitter, "x9", "_str_offset_msg");                 // the destination buffer
    abi::emit_symbol_address(emitter, "x4", "_str_offset_warn_prefix");         // the fixed text
    emitter.instruction("mov x10, #0");                                         // bytes written so far

    // -- the fixed prefix --
    emitter.label("__rt_wuso_prefix");
    emitter.instruction(&format!("cmp x10, #{prefix_len}"));
    emitter.instruction("b.hs __rt_wuso_sign");
    emitter.instruction("ldrb w12, [x4, x10]");
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("b __rt_wuso_prefix");

    // -- a leading '-' for a negative offset, which php prints as the caller wrote it --
    emitter.label("__rt_wuso_sign");
    emitter.instruction("tbz x3, #63, __rt_wuso_digits");                       // non-negative: straight to the digits
    emitter.instruction("mov w12, #45");                                        // ASCII '-'
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x10, x10, #1");
    // Negating through the unsigned domain is deliberate: for PHP_INT_MIN the bit pattern is
    // unchanged and reads as 2^63 unsigned, which is exactly its magnitude.
    emitter.instruction("neg x3, x3");

    // -- digits, produced least-significant first into the stack window --
    emitter.label("__rt_wuso_digits");
    emitter.instruction("add x5, sp, #24");                                     // one past the end of the window
    emitter.instruction("mov x6, x5");                                          // the backwards write cursor
    emitter.instruction("mov x7, #10");
    emitter.label("__rt_wuso_digit_loop");
    emitter.instruction("udiv x8, x3, x7");                                     // quotient
    emitter.instruction("msub x12, x8, x7, x3");                                // remainder = value - quotient * 10
    emitter.instruction("add x12, x12, #48");                                   // to ASCII
    emitter.instruction("sub x6, x6, #1");
    emitter.instruction("strb w12, [x6]");
    emitter.instruction("mov x3, x8");
    emitter.instruction("cbnz x3, __rt_wuso_digit_loop");                       // a do-while, so zero still writes '0'

    // -- copy the digits forward into the message --
    emitter.label("__rt_wuso_copy");
    emitter.instruction("cmp x6, x5");
    emitter.instruction("b.hs __rt_wuso_newline");
    emitter.instruction("ldrb w12, [x6]");
    emitter.instruction("strb w12, [x9, x10]");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("b __rt_wuso_copy");

    emitter.label("__rt_wuso_newline");
    emitter.instruction("mov w12, #10");                                        // elephc diagnostics end at the newline;
    emitter.instruction("strb w12, [x9, x10]");                                 // the " in FILE on line N" suffix is absent throughout
    emitter.instruction("add x10, x10, #1");

    emitter.instruction("mov x1, x9");                                          // message pointer
    emitter.instruction("mov x2, x10");                                         // message length
    emitter.instruction("bl __rt_diag_warning");                                // honours `@` like every other diagnostic

    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
}

/// Emits the x86_64 composer.
///
/// r8 = value, r9 = message buffer, r10 = prefix, r11 = bytes written, rsi/rdi = digit window
/// end and cursor. r12-r15 and rbx belong to the caller under SysV and are left alone.
fn emit_x86_64(emitter: &mut Emitter) {
    let prefix_len = STRING_OFFSET_PREFIX.len();
    emitter.blank();
    emitter.comment("--- runtime: compose the uninitialized-string-offset warning ---");
    emitter.label_global("__rt_warn_uninitialized_string_offset");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 48");                                         // the backwards digit window plus alignment

    emitter.instruction("mov r8, rdi");                                         // hold the caller's offset

    abi::emit_symbol_address(emitter, "r9", "_str_offset_msg");                 // the destination buffer
    abi::emit_symbol_address(emitter, "r10", "_str_offset_warn_prefix");        // the fixed text
    emitter.instruction("xor r11, r11");                                        // bytes written so far

    emitter.label("__rt_wuso_prefix");
    emitter.instruction(&format!("cmp r11, {prefix_len}"));
    emitter.instruction("jae __rt_wuso_sign");
    emitter.instruction("movzx ecx, BYTE PTR [r10 + r11]");
    emitter.instruction("mov BYTE PTR [r9 + r11], cl");
    emitter.instruction("add r11, 1");
    emitter.instruction("jmp __rt_wuso_prefix");

    emitter.label("__rt_wuso_sign");
    emitter.instruction("test r8, r8");
    emitter.instruction("jns __rt_wuso_digits");                                // non-negative: straight to the digits
    emitter.instruction("mov BYTE PTR [r9 + r11], 45");                         // ASCII '-'
    emitter.instruction("add r11, 1");
    // See the AArch64 half: negating through the unsigned domain is correct for PHP_INT_MIN.
    emitter.instruction("neg r8");

    emitter.label("__rt_wuso_digits");
    emitter.instruction("lea rsi, [rbp - 8]");                                  // one past the end of the window
    emitter.instruction("mov rdi, rsi");                                        // the backwards write cursor
    emitter.label("__rt_wuso_digit_loop");
    emitter.instruction("mov rax, r8");
    emitter.instruction("xor edx, edx");                                        // div reads rdx:rax, so clear the high half
    emitter.instruction("mov rcx, 10");
    emitter.instruction("div rcx");                                             // rax = quotient, rdx = remainder
    emitter.instruction("add rdx, 48");                                         // to ASCII
    emitter.instruction("sub rdi, 1");
    emitter.instruction("mov BYTE PTR [rdi], dl");
    emitter.instruction("mov r8, rax");
    emitter.instruction("test r8, r8");
    emitter.instruction("jnz __rt_wuso_digit_loop");                            // a do-while, so zero still writes '0'

    emitter.label("__rt_wuso_copy");
    emitter.instruction("cmp rdi, rsi");
    emitter.instruction("jae __rt_wuso_newline");
    emitter.instruction("movzx ecx, BYTE PTR [rdi]");
    emitter.instruction("mov BYTE PTR [r9 + r11], cl");
    emitter.instruction("add rdi, 1");
    emitter.instruction("add r11, 1");
    emitter.instruction("jmp __rt_wuso_copy");

    emitter.label("__rt_wuso_newline");
    emitter.instruction("mov BYTE PTR [r9 + r11], 10");                         // elephc diagnostics end at the newline
    emitter.instruction("add r11, 1");

    emitter.instruction("mov rdi, r9");                                         // message pointer
    emitter.instruction("mov rsi, r11");                                        // message length
    emitter.instruction("call __rt_diag_warning");                              // honours `@` like every other diagnostic

    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}
