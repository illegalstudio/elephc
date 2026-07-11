//! Purpose:
//! Emits `__rt_mb_strlen`, the runtime helper for PHP's `mb_strlen()`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()`.
//! - The `mb_strlen()` builtin lowering
//!   (`crate::codegen::lower_inst::builtins::strings::lower_mb_strlen`).
//!
//! Key details:
//! - Input string ptr/len arrive in x1/x2 (AArch64) or rax/edx (x86_64); the result is
//!   returned in x0 / rax (matching the `__rt_crc32` string-helper ABI).
//! - Counts UTF-8 code points: every byte whose top two bits are NOT `10` is a code-point
//!   leading byte; continuation bytes (`10xxxxxx`) are skipped. Matches `mb_strlen($s)`
//!   under UTF-8. The optional encoding argument is not supported by the lowering.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits `__rt_mb_strlen(str_ptr, str_len) -> count`, the UTF-8 code-point counter.
pub fn emit_mb_strlen(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mb_strlen_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mb_strlen (UTF-8 code-point count) ---");
    emitter.label_global("__rt_mb_strlen");
    emitter.instruction("mov x0, #0"); // count = 0 (return register)
    emitter.instruction("mov x4, #0"); // byte index = 0
    emitter.label("__rt_mb_strlen_loop");
    emitter.instruction("cmp x4, x2"); // processed every input byte? (len in x2)
    emitter.instruction("b.hs __rt_mb_strlen_done");
    emitter.instruction("ldrb w5, [x1, x4]"); // load the current byte (ptr in x1)
    emitter.instruction("and w5, w5, #0xC0"); // isolate the top two bits
    emitter.instruction("cmp w5, #0x80"); // continuation byte (10xxxxxx)?
    emitter.instruction("b.eq __rt_mb_strlen_skip"); // yes → not a code-point start
    emitter.instruction("add x0, x0, #1"); // count a code-point-leading byte
    emitter.label("__rt_mb_strlen_skip");
    emitter.instruction("add x4, x4, #1"); // advance to the next byte
    emitter.instruction("b __rt_mb_strlen_loop");
    emitter.label("__rt_mb_strlen_done");
    emitter.instruction("ret");
}

/// Emits the Linux x86_64 variant of `__rt_mb_strlen`.
fn emit_mb_strlen_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_strlen (UTF-8 code-point count) ---");
    emitter.label_global("__rt_mb_strlen");
    emitter.instruction("mov rsi, rax"); // rsi = input pointer (rax/edx hold ptr/len on entry)
    emitter.instruction("xor eax, eax"); // count = 0 (return register)
    emitter.instruction("xor r8, r8"); // byte index = 0
    emitter.label("__rt_mb_strlen_loop_x86");
    emitter.instruction("cmp r8d, edx"); // processed every input byte? (len in edx)
    emitter.instruction("jae __rt_mb_strlen_done_x86");
    emitter.instruction("movzx r9d, BYTE PTR [rsi + r8]"); // load the current byte
    emitter.instruction("and r9d, 0xC0"); // isolate the top two bits
    emitter.instruction("cmp r9d, 0x80"); // continuation byte (10xxxxxx)?
    emitter.instruction("je __rt_mb_strlen_skip_x86"); // yes → not a code-point start
    emitter.instruction("inc eax"); // count a code-point-leading byte
    emitter.label("__rt_mb_strlen_skip_x86");
    emitter.instruction("inc r8"); // advance to the next byte
    emitter.instruction("jmp __rt_mb_strlen_loop_x86");
    emitter.label("__rt_mb_strlen_done_x86");
    emitter.instruction("ret");
}
