//! Purpose:
//! Emits `__rt_crc32`, the runtime helper for PHP's `crc32()` — a pure
//! (table-free, bit-by-bit) CRC-32/ISO-HDLC computation over the input string,
//! returning the 32-bit checksum as a zero-extended, non-negative integer.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` and the minimal x86
//!   runtime via `crate::codegen_support::runtime::strings`.
//! - The EIR `crc32()` builtin lowerer in `crate::codegen::lower_inst::builtins::strings`.
//!
//! Key details:
//! - Input string ptr/len arrive in x1/x2 (AArch64) or rax/edx (x86_64); the
//!   result is returned in x0 / rax. Uses the reflected polynomial 0xEDB88320
//!   with init and final XOR of 0xFFFFFFFF — the standard zlib/PHP CRC-32.
//! - Writing the 32-bit result register zero-extends, so the value is always a
//!   non-negative PHP int in `0 ..= 0xFFFF_FFFF` (matching 64-bit PHP).

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_crc32(str_ptr, str_len) -> crc`, the CRC-32 checksum helper.
pub fn emit_crc32(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_crc32_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: crc32 ---");
    emitter.label_global("__rt_crc32");
    emitter.instruction("mov w3, #-1"); // crc = 0xFFFFFFFF (CRC-32 initial value)
    emitter.instruction("mov x4, #0"); // byte index = 0
    emitter.label("__rt_crc32_loop");
    emitter.instruction("cmp x4, x2"); // processed every input byte?
    emitter.instruction("b.hs __rt_crc32_done"); // yes → apply the final XOR
    emitter.instruction("ldrb w5, [x1, x4]"); // load the current input byte
    emitter.instruction("eor w3, w3, w5"); // fold the byte into the low 8 bits of crc
    emitter.instruction("mov w6, #8"); // 8 bits to process for this byte
    emitter.label("__rt_crc32_bit");
    emitter.instruction("tst w3, #1"); // is the low bit of crc set?
    emitter.instruction("lsr w7, w3, #1"); // tentative crc >> 1 (flags from tst preserved)
    emitter.instruction("b.eq __rt_crc32_skip"); // low bit clear → no polynomial XOR
    emitter.instruction("movz w8, #0x8320"); // low half of reflected polynomial 0xEDB88320
    emitter.instruction("movk w8, #0xEDB8, lsl #16"); // high half of reflected polynomial
    emitter.instruction("eor w7, w7, w8"); // crc = (crc >> 1) ^ 0xEDB88320
    emitter.label("__rt_crc32_skip");
    emitter.instruction("mov w3, w7"); // commit the updated crc
    emitter.instruction("subs w6, w6, #1"); // one fewer bit to process
    emitter.instruction("b.ne __rt_crc32_bit"); // loop over the remaining bits
    emitter.instruction("add x4, x4, #1"); // advance to the next byte
    emitter.instruction("b __rt_crc32_loop"); // continue the byte loop
    emitter.label("__rt_crc32_done");
    emitter.instruction("mvn w0, w3"); // result = crc ^ 0xFFFFFFFF; w-write zero-extends x0
    emitter.instruction("ret"); // return the non-negative 32-bit checksum
}

/// Emits the Linux x86_64 variant of `__rt_crc32`.
fn emit_crc32_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: crc32 ---");
    emitter.label_global("__rt_crc32");
    emitter.instruction("mov rsi, rax"); // rsi = input pointer (rax/edx hold ptr/len on entry)
    emitter.instruction("mov ecx, -1"); // crc = 0xFFFFFFFF (CRC-32 initial value)
    emitter.instruction("xor r8, r8"); // byte index = 0
    emitter.label("__rt_crc32_loop_x86");
    emitter.instruction("cmp r8d, edx"); // processed every input byte?
    emitter.instruction("jae __rt_crc32_done_x86"); // yes → apply the final XOR
    emitter.instruction("movzx r9d, BYTE PTR [rsi + r8]"); // load the current input byte
    emitter.instruction("xor ecx, r9d"); // fold the byte into the low 8 bits of crc
    emitter.instruction("mov r10d, 8"); // 8 bits to process for this byte
    emitter.label("__rt_crc32_bit_x86");
    emitter.instruction("mov r11d, ecx"); // tentative crc >> 1
    emitter.instruction("shr r11d, 1"); // shift crc right one bit
    emitter.instruction("test ecx, 1"); // is the low bit of crc set?
    emitter.instruction("je __rt_crc32_skip_x86"); // low bit clear → no polynomial XOR
    emitter.instruction("xor r11d, 0xEDB88320"); // crc = (crc >> 1) ^ 0xEDB88320
    emitter.label("__rt_crc32_skip_x86");
    emitter.instruction("mov ecx, r11d"); // commit the updated crc
    emitter.instruction("dec r10d"); // one fewer bit to process
    emitter.instruction("jne __rt_crc32_bit_x86"); // loop over the remaining bits
    emitter.instruction("inc r8"); // advance to the next byte
    emitter.instruction("jmp __rt_crc32_loop_x86"); // continue the byte loop
    emitter.label("__rt_crc32_done_x86");
    emitter.instruction("mov eax, ecx"); // move crc into the result register (zero-extends rax)
    emitter.instruction("not eax"); // result = crc ^ 0xFFFFFFFF
    emitter.instruction("ret"); // return the non-negative 32-bit checksum
}
