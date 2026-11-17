//! Purpose:
//! Emits `__rt_crc32`, the runtime helper for PHP's `crc32()` — a
//! slicing-by-8 CRC-32/ISO-HDLC computation over the input string, returning
//! the 32-bit checksum as a zero-extended, non-negative integer.
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
//! - The 8 KiB `_crc32_table8` holds the eight slicing tables; the byte-wise
//!   `_crc32_table` handles the 0-7 byte tail. Both are emitted by
//!   `crate::codegen_support::runtime::data::fixed`.
//! - Writing the 32-bit result register zero-extends, so the value is always a
//!   non-negative PHP int in `0 ..= 0xFFFF_FFFF` (matching 64-bit PHP).

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// CRC-32/ISO-HDLC lookup table, the standard reflected `0xEDB88320` table.
pub const CRC32_TABLE: [u32; 256] = build_crc32_table();

/// Slicing-by-8 tables: `CRC32_TABLE8[k]` advances a reflected CRC by `k + 1`
/// bytes when indexed by the corresponding byte of the folded input word.
pub const CRC32_TABLE8: [[u32; 256]; 8] = build_crc32_table8();

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

/// Builds the eight slicing-by-8 tables from the base table at compile time.
const fn build_crc32_table8() -> [[u32; 256]; 8] {
    let base = build_crc32_table();
    let mut tables = [[0u32; 256]; 8];
    tables[0] = base;
    let mut table = 1;
    while table < 8 {
        let mut index = 0;
        while index < 256 {
            let previous = tables[table - 1][index];
            tables[table][index] = (previous >> 8) ^ base[(previous & 0xff) as usize];
            index += 1;
        }
        table += 1;
    }
    tables
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
    emitter.instruction("cmp x2, #8");                                          // is there a full eight-byte block?
    emitter.instruction("b.lo __rt_crc32_tail");                                // short inputs use the byte-wise table
    abi::emit_symbol_address(emitter, "x9", "_crc32_table8");                   // eight slicing-by-8 tables
    emitter.instruction("add x11, x9, #4096");                                  // table 7 base (4 KiB + 3 KiB)
    emitter.instruction("add x11, x11, #3072");                                 // table 7 base (7 * 1024)
    emitter.instruction("add x12, x9, #4096");                                  // table 6 base (4 KiB + 2 KiB)
    emitter.instruction("add x12, x12, #2048");                                 // table 6 base (6 * 1024)
    emitter.instruction("add x13, x9, #4096");                                  // table 5 base (4 KiB + 1 KiB)
    emitter.instruction("add x13, x13, #1024");                                 // table 5 base (5 * 1024)
    emitter.instruction("add x14, x9, #4096");                                  // table 4 base (4 * 1024)
    emitter.instruction("add x15, x9, #2048");                                  // table 3 base (2 KiB + 1 KiB)
    emitter.instruction("add x15, x15, #1024");                                 // table 3 base (3 * 1024)
    emitter.instruction("add x16, x9, #2048");                                  // table 2 base (2 * 1024)
    emitter.instruction("add x17, x9, #1024");                                  // table 1 base (1 * 1024)
    emitter.instruction("sub x10, x2, #8");                                     // last index that starts a full block
    emitter.label("__rt_crc32_block");
    emitter.instruction("ldr x5, [x1, x4]");                                    // load eight input bytes (little-endian)
    emitter.instruction("eor w6, w3, w5");                                      // fold the low word into the running crc
    emitter.instruction("and w7, w6, #0xff");                                   // index table 7 with crc byte 0
    emitter.instruction("ldr w0, [x11, w7, uxtw #2]");                          // table 7 lookup
    emitter.instruction("ubfx w7, w6, #8, #8");                                 // extract crc byte 1
    emitter.instruction("ldr w8, [x12, w7, uxtw #2]");                          // table 6 lookup
    emitter.instruction("eor w0, w0, w8");                                      // combine table 7 and 6
    emitter.instruction("ubfx w7, w6, #16, #8");                                // extract crc byte 2
    emitter.instruction("ldr w8, [x13, w7, uxtw #2]");                          // table 5 lookup
    emitter.instruction("eor w0, w0, w8");                                      // combine table 5
    emitter.instruction("ubfx w7, w6, #24, #8");                                // extract crc byte 3
    emitter.instruction("ldr w8, [x14, w7, uxtw #2]");                          // table 4 lookup
    emitter.instruction("eor w0, w0, w8");                                      // combine table 4
    emitter.instruction("lsr x5, x5, #32");                                     // move input bytes 4..7 into the low word
    emitter.instruction("and w7, w5, #0xff");                                   // index table 3 with input byte 4
    emitter.instruction("ldr w8, [x15, w7, uxtw #2]");                          // table 3 lookup
    emitter.instruction("eor w0, w0, w8");                                      // combine table 3
    emitter.instruction("ubfx w7, w5, #8, #8");                                 // extract input byte 5
    emitter.instruction("ldr w8, [x16, w7, uxtw #2]");                          // table 2 lookup
    emitter.instruction("eor w0, w0, w8");                                      // combine table 2
    emitter.instruction("ubfx w7, w5, #16, #8");                                // extract input byte 6
    emitter.instruction("ldr w8, [x17, w7, uxtw #2]");                          // table 1 lookup
    emitter.instruction("eor w0, w0, w8");                                      // combine table 1
    emitter.instruction("ubfx w7, w5, #24, #8");                                // extract input byte 7
    emitter.instruction("ldr w8, [x9, w7, uxtw #2]");                           // table 0 lookup
    emitter.instruction("eor w3, w0, w8");                                      // form the new crc
    emitter.instruction("add x4, x4, #8");                                      // advance past the block
    emitter.instruction("cmp x4, x10");                                         // is another full block available?
    emitter.instruction("b.ls __rt_crc32_block");                               // process the next block
    emitter.label("__rt_crc32_tail");
    abi::emit_symbol_address(emitter, "x9", "_crc32_table");                    // byte-wise table for the tail
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
    emitter.instruction("xor r8d, r8d");                                        // byte index = 0
    emitter.instruction("cmp edx, 8");                                          // is there a full eight-byte block?
    emitter.instruction("jb __rt_crc32_tail_x86");                              // short inputs use the byte-wise table
    abi::emit_symbol_address(emitter, "r9", "_crc32_table8");                   // eight slicing-by-8 tables
    emitter.instruction("lea r11, [rdx - 8]");                                  // last index that starts a full block
    emitter.label("__rt_crc32_block_x86");
    emitter.instruction("mov r10, QWORD PTR [rsi + r8]");                       // load eight input bytes (little-endian)
    emitter.instruction("xor ecx, r10d");                                       // fold the low word into the running crc
    emitter.instruction("mov eax, ecx");                                        // copy the folded crc for byte extraction
    emitter.instruction("and eax, 0xff");                                       // index table 7 with crc byte 0
    emitter.instruction("mov eax, DWORD PTR [r9 + rax * 4 + 7168]");            // table 7 lookup
    emitter.instruction("mov edi, ecx");                                        // copy the folded crc for byte extraction
    emitter.instruction("shr edi, 8");                                          // extract crc byte 1
    emitter.instruction("movzx edi, dil");                                      // isolate crc byte 1
    emitter.instruction("xor eax, DWORD PTR [r9 + rdi * 4 + 6144]");            // combine table 6
    emitter.instruction("mov edi, ecx");                                        // copy the folded crc for byte extraction
    emitter.instruction("shr edi, 16");                                         // extract crc byte 2
    emitter.instruction("movzx edi, dil");                                      // isolate crc byte 2
    emitter.instruction("xor eax, DWORD PTR [r9 + rdi * 4 + 5120]");            // combine table 5
    emitter.instruction("mov edi, ecx");                                        // copy the folded crc for byte extraction
    emitter.instruction("shr edi, 24");                                         // extract crc byte 3
    emitter.instruction("movzx edi, dil");                                      // isolate crc byte 3
    emitter.instruction("xor eax, DWORD PTR [r9 + rdi * 4 + 4096]");            // combine table 4
    emitter.instruction("shr r10, 32");                                         // move input bytes 4..7 into the low word
    emitter.instruction("movzx edi, r10b");                                     // isolate input byte 4
    emitter.instruction("xor eax, DWORD PTR [r9 + rdi * 4 + 3072]");            // combine table 3
    emitter.instruction("mov edi, r10d");                                       // copy the input word for byte extraction
    emitter.instruction("shr edi, 8");                                          // extract input byte 5
    emitter.instruction("movzx edi, dil");                                      // isolate input byte 5
    emitter.instruction("xor eax, DWORD PTR [r9 + rdi * 4 + 2048]");            // combine table 2
    emitter.instruction("mov edi, r10d");                                       // copy the input word for byte extraction
    emitter.instruction("shr edi, 16");                                         // extract input byte 6
    emitter.instruction("movzx edi, dil");                                      // isolate input byte 6
    emitter.instruction("xor eax, DWORD PTR [r9 + rdi * 4 + 1024]");            // combine table 1
    emitter.instruction("mov edi, r10d");                                       // copy the input word for byte extraction
    emitter.instruction("shr edi, 24");                                         // extract input byte 7
    emitter.instruction("movzx edi, dil");                                      // isolate input byte 7
    emitter.instruction("xor eax, DWORD PTR [r9 + rdi * 4]");                   // combine table 0
    emitter.instruction("mov ecx, eax");                                        // form the new crc
    emitter.instruction("add r8, 8");                                           // advance past the block
    emitter.instruction("cmp r8, r11");                                         // is another full block available?
    emitter.instruction("jbe __rt_crc32_block_x86");                            // process the next block
    emitter.label("__rt_crc32_tail_x86");
    abi::emit_symbol_address(emitter, "r9", "_crc32_table");                    // byte-wise table for the tail
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

    /// Verifies both targets use the slicing-by-8 path and retain the byte-wise tail table.
    #[test]
    fn crc32_emits_the_slicing_tables_on_all_targets() {
        for name in ["macos-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).expect("supported target");
            let mut emitter = Emitter::new(target);
            emit_crc32(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains("_crc32_table8"), "{name}: {assembly}");
            assert!(assembly.contains("_crc32_table"), "{name}: {assembly}");
            assert!(!assembly.contains("__rt_crc32_bit"), "{name}: {assembly}");
        }
    }

    /// Computes the byte-wise reference CRC-32 for a slice.
    fn crc32_bytewise(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in bytes {
            crc = (crc >> 8) ^ CRC32_TABLE[((crc ^ u32::from(byte)) & 0xff) as usize];
        }
        crc ^ 0xFFFF_FFFF
    }

    /// Computes CRC-32 through the derived slicing tables, mirroring the emitted helper.
    fn crc32_slice8(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        let mut offset = 0;
        while offset + 8 <= bytes.len() {
            let word = u64::from_le_bytes(
                bytes[offset..offset + 8]
                    .try_into()
                    .expect("eight-byte window"),
            );
            let folded = crc ^ word as u32;
            crc = CRC32_TABLE8[7][(folded & 0xff) as usize]
                ^ CRC32_TABLE8[6][((folded >> 8) & 0xff) as usize]
                ^ CRC32_TABLE8[5][((folded >> 16) & 0xff) as usize]
                ^ CRC32_TABLE8[4][((folded >> 24) & 0xff) as usize]
                ^ CRC32_TABLE8[3][bytes[offset + 4] as usize]
                ^ CRC32_TABLE8[2][bytes[offset + 5] as usize]
                ^ CRC32_TABLE8[1][bytes[offset + 6] as usize]
                ^ CRC32_TABLE8[0][bytes[offset + 7] as usize];
            offset += 8;
        }
        for &byte in &bytes[offset..] {
            crc = (crc >> 8) ^ CRC32_TABLE[((crc ^ u32::from(byte)) & 0xff) as usize];
        }
        crc ^ 0xFFFF_FFFF
    }

    /// Verifies the derived slicing tables reproduce the byte-wise CRC across block and tail lengths.
    #[test]
    fn crc32_slicing_tables_match_the_bytewise_crc() {
        let long = vec![b'a'; 129];
        let bytes: Vec<u8> = (0..=255).collect();
        for input in [
            &b""[..],
            &b"123456789"[..],
            &b"The quick brown fox"[..],
            &b"abc\0def"[..],
            &bytes[..],
            &long[..],
        ] {
            assert_eq!(crc32_slice8(input), crc32_bytewise(input), "{input:?}");
        }
        assert_eq!(crc32_slice8(b"123456789"), 0xCBF4_3926);
    }
}
