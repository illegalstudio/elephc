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

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// CRC-32/ISO-HDLC lookup table, the standard reflected `0xEDB88320` table.
pub const CRC32_TABLE: [u32; 256] = build_crc32_table();

/// Builds the 256-entry CRC-32 table at compile time.
const fn build_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
}

/// Emits `__rt_crc32(str_ptr, str_len) -> crc`, the CRC-32 checksum helper.
pub fn emit_crc32(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_crc32_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: crc32 ---");
    emitter.label_global("__rt_crc32");
    emitter.instruction("mov w3, #-1");                                         // crc = 0xFFFFFFFF (CRC-32 initial value)
    emitter.instruction("mov x4, #0");                                          // byte index = 0
    abi::emit_symbol_address(emitter, "x9", "_crc32_table");                    // table base for the reflected polynomial
    emitter.label("__rt_crc32_loop");
    emitter.instruction("cmp x4, x2");                                          // processed every input byte?
    emitter.instruction("b.hs __rt_crc32_done");                                // yes → apply the final XOR
    emitter.instruction("ldrb w5, [x1, x4]");                                   // load the current input byte
    emitter.instruction("eor w5, w3, w5");                                      // fold the byte into the low 8 bits of crc
    emitter.instruction("and w5, w5, #0xff");                                   // index the table by the low byte
    emitter.instruction("ldr w6, [x9, x5, lsl #2]");                            // load the next table entry
    emitter.instruction("lsr w3, w3, #8");                                      // drop the low byte of the running crc
    emitter.instruction("eor w3, w3, w6");                                      // apply the table entry
    emitter.instruction("add x4, x4, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_crc32_loop");                                   // continue the byte loop
    emitter.label("__rt_crc32_done");
    emitter.instruction("mvn w0, w3");                                          // result = crc ^ 0xFFFFFFFF; w-write zero-extends x0
    emitter.instruction("ret");                                                 // return the non-negative 32-bit checksum
}

/// Emits the Linux x86_64 variant of `__rt_crc32`.
fn emit_crc32_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: crc32 ---");
    emitter.label_global("__rt_crc32");
    emitter.instruction("mov rsi, rax");                                        // rsi = input pointer (rax/edx hold ptr/len on entry)
    emitter.instruction("mov ecx, -1");                                         // crc = 0xFFFFFFFF (CRC-32 initial value)
    emitter.instruction("xor r8, r8");                                          // byte index = 0
    abi::emit_symbol_address(emitter, "r9", "_crc32_table");                    // table base for the reflected polynomial
    emitter.label("__rt_crc32_loop_x86");
    emitter.instruction("cmp r8d, edx");                                        // processed every input byte?
    emitter.instruction("jae __rt_crc32_done_x86");                             // yes → apply the final XOR
    emitter.instruction("movzx r10d, BYTE PTR [rsi + r8]");                     // load the current input byte
    emitter.instruction("xor r10d, ecx");                                       // fold the byte into the low 8 bits of crc
    emitter.instruction("and r10d, 0xff");                                      // index the table by the low byte
    emitter.instruction("mov r11d, DWORD PTR [r9 + r10 * 4]");                  // load the next table entry
    emitter.instruction("shr ecx, 8");                                          // drop the low byte of the running crc
    emitter.instruction("xor ecx, r11d");                                       // apply the table entry
    emitter.instruction("inc r8");                                              // advance to the next byte
    emitter.instruction("jmp __rt_crc32_loop_x86");                             // continue the byte loop
    emitter.label("__rt_crc32_done_x86");
    emitter.instruction("mov eax, ecx");                                        // move crc into the result register (zero-extends rax)
    emitter.instruction("not eax");                                             // result = crc ^ 0xFFFFFFFF
    emitter.instruction("ret");                                                 // return the non-negative 32-bit checksum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Verifies both targets use the table-driven loop rather than the old bit loop.
    #[test]
    fn crc32_emits_the_lookup_table_path_on_all_targets() {
        for name in ["macos-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).expect("supported target");
            let mut emitter = Emitter::new(target);
            emit_crc32(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains("_crc32_table"), "{name}: {assembly}");
            assert!(!assembly.contains("__rt_crc32_bit"), "{name}: {assembly}");
        }
    }
}
