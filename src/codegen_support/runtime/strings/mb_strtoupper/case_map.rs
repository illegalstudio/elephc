//! Purpose:
//! Emits Unicode full-uppercase lookup tables and the shared `__rt_mb_case_upper` helper.
//!
//! Called from:
//! - `super::emit_mb_strtoupper()`.
//!
//! Key details:
//! - Tables are generated from Rust's `char::to_uppercase()`, which matches PHP 8.5
//!   language-agnostic full case mapping (`ß` → `SS`).
//! - The helper writes 1-3 UTF-32 code points to a caller-supplied buffer and returns the count.

use std::sync::OnceLock;

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Simple 1:1 uppercase mappings plus special 1:N mappings used by the runtime helper.
type CaseTables = (Vec<(u32, u32)>, Vec<(u32, [u32; 3], u8)>);

/// Builds and caches the Unicode uppercase tables used by the emitted runtime helper.
fn unicode_uppercase_tables() -> &'static CaseTables {
    static TABLES: OnceLock<CaseTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut simple = Vec::new();
        let mut special = Vec::new();
        for cp in 0..=0x10FFFF {
            let Some(ch) = char::from_u32(cp) else {
                continue;
            };
            let mapped: Vec<char> = ch.to_uppercase().collect();
            if mapped.len() == 1 && mapped[0] as u32 == cp {
                continue;
            }
            if mapped.len() == 1 {
                simple.push((cp, mapped[0] as u32));
                continue;
            }
            let mut chars = [0u32; 3];
            for (index, upper) in mapped.iter().take(3).enumerate() {
                chars[index] = *upper as u32;
            }
            special.push((cp, chars, mapped.len() as u8));
        }
        (simple, special)
    })
}

/// Emits the `.data` uppercase tables used by `__rt_mb_case_upper`.
pub(super) fn emit_case_map_data(emitter: &mut Emitter) {
    let (simple, special) = unicode_uppercase_tables();
    emitter.blank();
    emitter.raw(".data");
    emitter.raw(".align 8");
    emitter.raw(".globl _mb_strtoupper_simple_map");
    emitter.raw("_mb_strtoupper_simple_map:");
    for (from, to) in simple {
        emitter.raw(&format!("    .long {}, {}", from, to));
    }
    emitter.raw(".globl _mb_strtoupper_simple_map_len");
    emitter.raw("_mb_strtoupper_simple_map_len:");
    emitter.raw(&format!("    .quad {}", simple.len()));
    emitter.raw(".globl _mb_strtoupper_special_map");
    emitter.raw("_mb_strtoupper_special_map:");
    for (from, chars, count) in special {
        emitter.raw(&format!(
            "    .long {}, {}, {}, {}, {}, 0",
            from, count, chars[0], chars[1], chars[2]
        ));
    }
    emitter.raw(".globl _mb_strtoupper_special_map_len");
    emitter.raw("_mb_strtoupper_special_map_len:");
    emitter.raw(&format!("    .quad {}", special.len()));
}

/// Emits `__rt_mb_case_upper` for the active target.
pub(super) fn emit_case_upper(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_case_upper_x86_64(emitter);
    } else {
        emit_case_upper_aarch64(emitter);
    }
}

/// Emits the AArch64 binary-search / special-case uppercase helper.
///
/// Input: `x0` = code point, `x1` = destination `u32` buffer.
/// Output: `x0` = written count (1-3). Clobbers `x2`-`x8`.
fn emit_case_upper_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unicode uppercase lookup ---");
    emitter.label_global("__rt_mb_case_upper");
    emitter.instruction("mov w2, w0");                                          // preserve the source code point while searching the tables
    abi::emit_symbol_address(emitter, "x3", "_mb_strtoupper_simple_map");
    abi::emit_symbol_address(emitter, "x4", "_mb_strtoupper_simple_map_len");
    emitter.instruction("ldr x4, [x4]");                                        // load the number of 1:1 uppercase mappings
    emitter.instruction("mov x5, #0");                                          // binary-search low index starts at zero
    emitter.label("__rt_mb_case_upper_simple_loop");
    emitter.instruction("cmp x5, x4");                                          // has the simple-map search range emptied?
    emitter.instruction("b.hs __rt_mb_case_upper_special");                     // miss the 1:1 table and inspect 1:N mappings
    emitter.instruction("add x6, x5, x4");                                      // mid = (lo + hi)
    emitter.instruction("lsr x6, x6, #1");                                      // mid = (lo + hi) / 2
    emitter.instruction("add x7, x3, x6, lsl #3");                              // address the 8-byte (from, to) entry
    emitter.instruction("ldr w8, [x7]");                                        // load the mapping's source code point
    emitter.instruction("cmp w8, w2");                                          // compare the table key with the query
    emitter.instruction("b.eq __rt_mb_case_upper_simple_found");                // a 1:1 mapping replaces the source scalar
    emitter.instruction("b.lo __rt_mb_case_upper_simple_right");                // search the upper half when the key is too small
    emitter.instruction("mov x4, x6");                                          // shrink hi to mid
    emitter.instruction("b __rt_mb_case_upper_simple_loop");                    // continue the 1:1 binary search
    emitter.label("__rt_mb_case_upper_simple_right");
    emitter.instruction("add x5, x6, #1");                                      // shrink lo to mid + 1
    emitter.instruction("b __rt_mb_case_upper_simple_loop");                    // continue the 1:1 binary search
    emitter.label("__rt_mb_case_upper_simple_found");
    emitter.instruction("ldr w8, [x7, #4]");                                    // load the mapped uppercase code point
    emitter.instruction("str w8, [x1]");                                        // write the single mapped scalar
    emitter.instruction("mov x0, #1");                                          // one output code point
    emitter.instruction("ret");                                                 // return the 1:1 mapping

    emitter.label("__rt_mb_case_upper_special");
    abi::emit_symbol_address(emitter, "x3", "_mb_strtoupper_special_map");
    abi::emit_symbol_address(emitter, "x4", "_mb_strtoupper_special_map_len");
    emitter.instruction("ldr x4, [x4]");                                        // load the number of 1:N uppercase mappings
    emitter.instruction("mov x5, #0");                                          // special-map index starts at zero
    emitter.label("__rt_mb_case_upper_special_loop");
    emitter.instruction("cmp x5, x4");                                          // inspected every special mapping?
    emitter.instruction("b.hs __rt_mb_case_upper_identity");                    // unmapped scalars stay unchanged
    emitter.instruction("mov x6, #24");                                         // each special entry is 24 bytes
    emitter.instruction("mul x7, x5, x6");                                      // byte offset of this special mapping
    emitter.instruction("add x7, x3, x7");                                      // address the special mapping
    emitter.instruction("ldr w8, [x7]");                                        // load the special mapping's source code point
    emitter.instruction("cmp w8, w2");                                          // does this special mapping match?
    emitter.instruction("b.eq __rt_mb_case_upper_special_found");               // write the expanded uppercase sequence
    emitter.instruction("add x5, x5, #1");                                      // advance to the next special mapping
    emitter.instruction("b __rt_mb_case_upper_special_loop");                   // continue the linear special-map scan
    emitter.label("__rt_mb_case_upper_special_found");
    emitter.instruction("ldr w0, [x7, #4]");                                    // load the expanded mapping length
    emitter.instruction("ldr w8, [x7, #8]");                                    // load the first uppercase scalar
    emitter.instruction("str w8, [x1]");                                        // write the first uppercase scalar
    emitter.instruction("ldr w8, [x7, #12]");                                   // load the second uppercase scalar
    emitter.instruction("str w8, [x1, #4]");                                    // write the second uppercase scalar
    emitter.instruction("ldr w8, [x7, #16]");                                   // load the third uppercase scalar
    emitter.instruction("str w8, [x1, #8]");                                    // write the third uppercase scalar
    emitter.instruction("ret");                                                 // return the 1:N mapping count
    emitter.label("__rt_mb_case_upper_identity");
    emitter.instruction("str w2, [x1]");                                        // write the original code point unchanged
    emitter.instruction("mov x0, #1");                                          // one output code point
    emitter.instruction("ret");                                                 // return the identity mapping
}

/// Emits the Linux x86_64 uppercase helper.
///
/// Input: `eax` = code point, `rdi` = destination `u32` buffer.
/// Output: `rax` = written count (1-3).
fn emit_case_upper_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unicode uppercase lookup ---");
    emitter.label_global("__rt_mb_case_upper");
    emitter.instruction("mov r8d, eax");                                        // preserve the source code point while searching the tables
    abi::emit_symbol_address(emitter, "r9", "_mb_strtoupper_simple_map");
    abi::emit_symbol_address(emitter, "r10", "_mb_strtoupper_simple_map_len");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the number of 1:1 uppercase mappings
    emitter.instruction("xor r11, r11");                                        // binary-search low index starts at zero
    emitter.label("__rt_mb_case_upper_simple_loop_x86");
    emitter.instruction("cmp r11, r10");                                        // has the simple-map search range emptied?
    emitter.instruction("jae __rt_mb_case_upper_special_x86");                  // miss the 1:1 table and inspect 1:N mappings
    emitter.instruction("lea rdx, [r11 + r10]");                                // mid = (lo + hi)
    emitter.instruction("shr rdx, 1");                                          // mid = (lo + hi) / 2
    emitter.instruction("mov ecx, DWORD PTR [r9 + rdx * 8]");                   // load the mapping's source code point
    emitter.instruction("cmp ecx, r8d");                                        // compare the table key with the query
    emitter.instruction("je __rt_mb_case_upper_simple_found_x86");              // a 1:1 mapping replaces the source scalar
    emitter.instruction("jb __rt_mb_case_upper_simple_right_x86");              // search the upper half when the key is too small
    emitter.instruction("mov r10, rdx");                                        // shrink hi to mid
    emitter.instruction("jmp __rt_mb_case_upper_simple_loop_x86");              // continue the 1:1 binary search
    emitter.label("__rt_mb_case_upper_simple_right_x86");
    emitter.instruction("lea r11, [rdx + 1]");                                  // shrink lo to mid + 1
    emitter.instruction("jmp __rt_mb_case_upper_simple_loop_x86");              // continue the 1:1 binary search
    emitter.label("__rt_mb_case_upper_simple_found_x86");
    emitter.instruction("mov ecx, DWORD PTR [r9 + rdx * 8 + 4]");               // load the mapped uppercase code point
    emitter.instruction("mov DWORD PTR [rdi], ecx");                            // write the single mapped scalar
    emitter.instruction("mov eax, 1");                                          // one output code point
    emitter.instruction("ret");                                                 // return the 1:1 mapping

    emitter.label("__rt_mb_case_upper_special_x86");
    abi::emit_symbol_address(emitter, "r9", "_mb_strtoupper_special_map");
    abi::emit_symbol_address(emitter, "r10", "_mb_strtoupper_special_map_len");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the number of 1:N uppercase mappings
    emitter.instruction("xor r11, r11");                                        // special-map index starts at zero
    emitter.label("__rt_mb_case_upper_special_loop_x86");
    emitter.instruction("cmp r11, r10");                                        // inspected every special mapping?
    emitter.instruction("jae __rt_mb_case_upper_identity_x86");                 // unmapped scalars stay unchanged
    emitter.instruction("imul rdx, r11, 24");                                   // byte offset of this special mapping
    emitter.instruction("mov ecx, DWORD PTR [r9 + rdx]");                       // load the special mapping's source code point
    emitter.instruction("cmp ecx, r8d");                                        // does this special mapping match?
    emitter.instruction("je __rt_mb_case_upper_special_found_x86");             // write the expanded uppercase sequence
    emitter.instruction("inc r11");                                             // advance to the next special mapping
    emitter.instruction("jmp __rt_mb_case_upper_special_loop_x86");             // continue the linear special-map scan
    emitter.label("__rt_mb_case_upper_special_found_x86");
    emitter.instruction("mov eax, DWORD PTR [r9 + rdx + 4]");                   // load the expanded mapping length
    emitter.instruction("mov ecx, DWORD PTR [r9 + rdx + 8]");                   // load the first uppercase scalar
    emitter.instruction("mov DWORD PTR [rdi], ecx");                            // write the first uppercase scalar
    emitter.instruction("mov ecx, DWORD PTR [r9 + rdx + 12]");                  // load the second uppercase scalar
    emitter.instruction("mov DWORD PTR [rdi + 4], ecx");                        // write the second uppercase scalar
    emitter.instruction("mov ecx, DWORD PTR [r9 + rdx + 16]");                  // load the third uppercase scalar
    emitter.instruction("mov DWORD PTR [rdi + 8], ecx");                        // write the third uppercase scalar
    emitter.instruction("ret");                                                 // return the 1:N mapping count
    emitter.label("__rt_mb_case_upper_identity_x86");
    emitter.instruction("mov DWORD PTR [rdi], r8d");                            // write the original code point unchanged
    emitter.instruction("mov eax, 1");                                          // one output code point
    emitter.instruction("ret");                                                 // return the identity mapping
}
