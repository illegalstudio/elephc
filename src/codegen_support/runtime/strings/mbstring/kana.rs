//! Purpose:
//! Emits the allocation-light half-width-to-full-width katakana converter used by the
//! explicit-UTF-8 `mb_convert_kana($s, "K")` fast path.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::mbstring::emit_mbstring()`.
//!
//! Key details:
//! - Only ASCII bytes and the half-width katakana block U+FF61..U+FF9D are accepted. Voiced
//!   sound marks (U+FF9E/U+FF9F) and every other codepoint return a null pointer so the shared
//!   mbstring bridge keeps PHP's combining and full-width-input behavior.
//! - Accepted half-width characters map to three-byte full-width codepoints, so the result
//!   keeps the source byte length and one owned allocation holds the converted bytes.
//! - AArch64 input is `x1`/`x2` = pointer/length; x86_64 input is `rax`/`rdx`.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the half-width katakana converter for the active target.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits the AArch64 half-width katakana converter.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_convert_kana_k_utf8 ---");
    emitter.label_global("__rt_mb_convert_kana_k_utf8");
    emitter.instruction("sub sp, sp, #64");                                     // reserve the source pair, destination, and linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                   // establish a stable frame base
    emitter.instruction("stp x1, x2, [sp, #0]");                               // save the source pointer and length
    emitter.instruction("mov x9, x1");                                         // pre-scan cursor
    emitter.instruction("mov x10, x2");                                       // remaining pre-scan bytes
    emitter.label("__rt_mb_convert_kana_k_utf8_scan");
    emitter.instruction("cbz x10, __rt_mb_convert_kana_k_utf8_supported");     // every byte is representable
    emitter.instruction("ldrb w11, [x9]");                                     // load the next source byte
    emitter.instruction("cmp w11, #0x80");                                     // is this a plain ASCII byte?
    emitter.instruction("b.lo __rt_mb_convert_kana_k_utf8_scan_ascii");        // copy ASCII unchanged
    emitter.instruction("cmp w11, #0xEF");                                     // is this the half-width katakana lead?
    emitter.instruction("b.ne __rt_mb_convert_kana_k_utf8_fallback");          // other codepoints stay on the bridge
    emitter.instruction("cmp x10, #3");                                        // is a complete three-byte sequence present?
    emitter.instruction("b.lo __rt_mb_convert_kana_k_utf8_fallback");          // a truncated sequence stays on the bridge
    emitter.instruction("ldrb w12, [x9, #1]");                                 // load the sequence's second byte
    emitter.instruction("ldrb w13, [x9, #2]");                                 // load the sequence's third byte
    emitter.instruction("cmp w12, #0xBD");                                     // is this the U+FF61..U+FF7F row?
    emitter.instruction("b.eq __rt_mb_convert_kana_k_utf8_scan_bd");           // check the first row's final byte
    emitter.instruction("cmp w12, #0xBE");                                     // is this the U+FF80..U+FF9F row?
    emitter.instruction("b.ne __rt_mb_convert_kana_k_utf8_fallback");          // other codepoints stay on the bridge
    emitter.instruction("cmp w13, #0x80");                                     // is this below the katakana row?
    emitter.instruction("b.lo __rt_mb_convert_kana_k_utf8_fallback");          // other codepoints stay on the bridge
    emitter.instruction("cmp w13, #0x9D");                                     // is this above the unvoiced katakana row?
    emitter.instruction("b.hi __rt_mb_convert_kana_k_utf8_fallback");          // voiced marks stay on the bridge
    emitter.instruction("add x9, x9, #3");                                     // consume the converted character
    emitter.instruction("sub x10, x10, #3");                                   // account for the converted character
    emitter.instruction("b __rt_mb_convert_kana_k_utf8_scan");                 // inspect the next character
    emitter.label("__rt_mb_convert_kana_k_utf8_scan_bd");
    emitter.instruction("cmp w13, #0xA1");                                     // is this below the katakana row?
    emitter.instruction("b.lo __rt_mb_convert_kana_k_utf8_fallback");          // other codepoints stay on the bridge
    emitter.instruction("cmp w13, #0xBF");                                     // is this above the katakana row?
    emitter.instruction("b.hi __rt_mb_convert_kana_k_utf8_fallback");          // other codepoints stay on the bridge
    emitter.instruction("add x9, x9, #3");                                     // consume the converted character
    emitter.instruction("sub x10, x10, #3");                                   // account for the converted character
    emitter.instruction("b __rt_mb_convert_kana_k_utf8_scan");                 // inspect the next character
    emitter.label("__rt_mb_convert_kana_k_utf8_scan_ascii");
    emitter.instruction("add x9, x9, #1");                                     // consume one ASCII byte
    emitter.instruction("sub x10, x10, #1");                                   // account for the consumed byte
    emitter.instruction("b __rt_mb_convert_kana_k_utf8_scan");                 // inspect the next character
    emitter.label("__rt_mb_convert_kana_k_utf8_supported");
    emitter.instruction("ldr x0, [sp, #8]");                                   // the converted result keeps the source byte length
    emitter.instruction("bl __rt_str_new_owned");                             // allocate one owned result
    emitter.instruction("str x0, [sp, #16]");                                  // save the destination base
    emitter.instruction("mov x9, x0");                                         // destination cursor
    emitter.instruction("ldr x1, [sp, #0]");                                   // reload the source cursor
    emitter.instruction("ldr x10, [sp, #8]");                                  // reload the remaining source bytes
    abi::emit_symbol_address(emitter, "x15", "_mb_halfwidth_kana_tbl");
    emitter.label("__rt_mb_convert_kana_k_utf8_copy");
    emitter.instruction("cbz x10, __rt_mb_convert_kana_k_utf8_done");          // stop after the final source byte
    emitter.instruction("ldrb w11, [x1]");                                     // load the next source byte
    emitter.instruction("cmp w11, #0x80");                                     // is this a plain ASCII byte?
    emitter.instruction("b.lo __rt_mb_convert_kana_k_utf8_copy_ascii");       // copy ASCII unchanged
    emitter.instruction("ldrb w12, [x1, #1]");                                 // load the sequence's second byte
    emitter.instruction("ldrb w13, [x1, #2]");                                 // load the sequence's third byte
    emitter.instruction("add x1, x1, #3");                                     // consume the half-width character
    emitter.instruction("sub x10, x10, #3");                                   // account for the consumed character
    emitter.instruction("cmp w12, #0xBD");                                     // is this the U+FF61..U+FF7F row?
    emitter.instruction("b.eq __rt_mb_convert_kana_k_utf8_index_bd");          // index the first table row
    emitter.instruction("sub w14, w13, #0x80");                                // index inside the U+FF80..U+FF9D row
    emitter.instruction("add w14, w14, #31");                                  // offset past the first table row
    emitter.instruction("b __rt_mb_convert_kana_k_utf8_index_ready");         // look up the mapped bytes
    emitter.label("__rt_mb_convert_kana_k_utf8_index_bd");
    emitter.instruction("sub w14, w13, #0xA1");                                // index inside the U+FF61..U+FF7F row
    emitter.label("__rt_mb_convert_kana_k_utf8_index_ready");
    emitter.instruction("add w14, w14, w14, lsl #1");                          // each table entry stores three bytes
    emitter.instruction("add x14, x15, x14");                                  // point at the mapped UTF-8 triple
    emitter.instruction("ldrb w11, [x14]");                                    // load the mapped first byte
    emitter.instruction("strb w11, [x9], #1");                                 // write the mapped first byte
    emitter.instruction("ldrb w11, [x14, #1]");                                // load the mapped second byte
    emitter.instruction("strb w11, [x9], #1");                                 // write the mapped second byte
    emitter.instruction("ldrb w11, [x14, #2]");                                // load the mapped third byte
    emitter.instruction("strb w11, [x9], #1");                                 // write the mapped third byte
    emitter.instruction("b __rt_mb_convert_kana_k_utf8_copy");                // continue with the next character
    emitter.label("__rt_mb_convert_kana_k_utf8_copy_ascii");
    emitter.instruction("add x1, x1, #1");                                     // consume one ASCII byte
    emitter.instruction("sub x10, x10, #1");                                   // account for the consumed byte
    emitter.instruction("strb w11, [x9], #1");                                 // copy the ASCII byte unchanged
    emitter.instruction("b __rt_mb_convert_kana_k_utf8_copy");                // continue with the next character
    emitter.label("__rt_mb_convert_kana_k_utf8_done");
    emitter.instruction("ldr x1, [sp, #16]");                                  // return the owned result pointer
    emitter.instruction("ldr x2, [sp, #8]");                                   // return the unchanged result length
    emitter.instruction("ldp x29, x30, [sp, #48]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                    // release the converter frame
    emitter.instruction("ret");                                               // transfer the owned converted string
    emitter.label("__rt_mb_convert_kana_k_utf8_fallback");
    emitter.instruction("mov x1, xzr");                                        // signal that the caller must use the bridge
    emitter.instruction("ldp x29, x30, [sp, #48]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                    // release the converter frame
    emitter.instruction("ret");                                               // return the fallback sentinel
}

/// Emits the x86_64 half-width katakana converter.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_convert_kana_k_utf8 ---");
    emitter.label_global("__rt_mb_convert_kana_k_utf8");
    emitter.instruction("push rbp");                                           // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                       // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                        // reserve the source pair, destination, and linkage
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                       // save the source pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                      // save the source length
    emitter.instruction("mov r8, rax");                                       // pre-scan cursor
    emitter.instruction("mov r9, rdx");                                       // remaining pre-scan bytes
    emitter.label("__rt_mb_convert_kana_k_utf8_scan_x86");
    emitter.instruction("test r9, r9");                                        // is the pre-scan complete?
    emitter.instruction("jz __rt_mb_convert_kana_k_utf8_supported_x86");       // every byte is representable
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                          // load the next source byte
    emitter.instruction("cmp ecx, 0x80");                                      // is this a plain ASCII byte?
    emitter.instruction("jb __rt_mb_convert_kana_k_utf8_scan_ascii_x86");      // copy ASCII unchanged
    emitter.instruction("cmp ecx, 0xEF");                                      // is this the half-width katakana lead?
    emitter.instruction("jne __rt_mb_convert_kana_k_utf8_fallback_x86");       // other codepoints stay on the bridge
    emitter.instruction("cmp r9, 3");                                          // is a complete three-byte sequence present?
    emitter.instruction("jb __rt_mb_convert_kana_k_utf8_fallback_x86");        // a truncated sequence stays on the bridge
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 1]");                       // load the sequence's second byte
    emitter.instruction("movzx edx, BYTE PTR [r8 + 2]");                       // load the sequence's third byte
    emitter.instruction("cmp ecx, 0xBD");                                      // is this the U+FF61..U+FF7F row?
    emitter.instruction("je __rt_mb_convert_kana_k_utf8_scan_bd_x86");         // check the first row's final byte
    emitter.instruction("cmp ecx, 0xBE");                                      // is this the U+FF80..U+FF9F row?
    emitter.instruction("jne __rt_mb_convert_kana_k_utf8_fallback_x86");       // other codepoints stay on the bridge
    emitter.instruction("cmp edx, 0x80");                                      // is this below the katakana row?
    emitter.instruction("jb __rt_mb_convert_kana_k_utf8_fallback_x86");        // other codepoints stay on the bridge
    emitter.instruction("cmp edx, 0x9D");                                      // is this above the unvoiced katakana row?
    emitter.instruction("ja __rt_mb_convert_kana_k_utf8_fallback_x86");        // voiced marks stay on the bridge
    emitter.instruction("add r8, 3");                                          // consume the converted character
    emitter.instruction("sub r9, 3");                                          // account for the converted character
    emitter.instruction("jmp __rt_mb_convert_kana_k_utf8_scan_x86");           // inspect the next character
    emitter.label("__rt_mb_convert_kana_k_utf8_scan_bd_x86");
    emitter.instruction("cmp edx, 0xA1");                                      // is this below the katakana row?
    emitter.instruction("jb __rt_mb_convert_kana_k_utf8_fallback_x86");        // other codepoints stay on the bridge
    emitter.instruction("cmp edx, 0xBF");                                      // is this above the katakana row?
    emitter.instruction("ja __rt_mb_convert_kana_k_utf8_fallback_x86");        // other codepoints stay on the bridge
    emitter.instruction("add r8, 3");                                          // consume the converted character
    emitter.instruction("sub r9, 3");                                          // account for the converted character
    emitter.instruction("jmp __rt_mb_convert_kana_k_utf8_scan_x86");           // inspect the next character
    emitter.label("__rt_mb_convert_kana_k_utf8_scan_ascii_x86");
    emitter.instruction("inc r8");                                            // consume one ASCII byte
    emitter.instruction("dec r9");                                            // account for the consumed byte
    emitter.instruction("jmp __rt_mb_convert_kana_k_utf8_scan_x86");           // inspect the next character
    emitter.label("__rt_mb_convert_kana_k_utf8_supported_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                      // the converted result keeps the source byte length
    emitter.instruction("call __rt_str_new_owned");                          // allocate one owned result
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                      // save the destination base
    emitter.instruction("mov r9, rax");                                       // destination cursor
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                        // reload the source cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                      // reload the remaining source bytes
    abi::emit_symbol_address(emitter, "r11", "_mb_halfwidth_kana_tbl");
    emitter.label("__rt_mb_convert_kana_k_utf8_copy_x86");
    emitter.instruction("test r10, r10");                                     // stop after the final source byte
    emitter.instruction("jz __rt_mb_convert_kana_k_utf8_done_x86");            // publish the owned result
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                          // load the next source byte
    emitter.instruction("cmp ecx, 0x80");                                      // is this a plain ASCII byte?
    emitter.instruction("jb __rt_mb_convert_kana_k_utf8_copy_ascii_x86");      // copy ASCII unchanged
    emitter.instruction("movzx edx, BYTE PTR [r8 + 1]");                       // load the sequence's second byte
    emitter.instruction("movzx esi, BYTE PTR [r8 + 2]");                       // load the sequence's third byte
    emitter.instruction("add r8, 3");                                          // consume the half-width character
    emitter.instruction("sub r10, 3");                                         // account for the consumed character
    emitter.instruction("cmp edx, 0xBD");                                      // is this the U+FF61..U+FF7F row?
    emitter.instruction("je __rt_mb_convert_kana_k_utf8_index_bd_x86");        // index the first table row
    emitter.instruction("sub esi, 0x80");                                      // index inside the U+FF80..U+FF9D row
    emitter.instruction("add esi, 31");                                       // offset past the first table row
    emitter.instruction("jmp __rt_mb_convert_kana_k_utf8_index_ready_x86");    // look up the mapped bytes
    emitter.label("__rt_mb_convert_kana_k_utf8_index_bd_x86");
    emitter.instruction("sub esi, 0xA1");                                      // index inside the U+FF61..U+FF7F row
    emitter.label("__rt_mb_convert_kana_k_utf8_index_ready_x86");
    emitter.instruction("lea rsi, [rsi + rsi * 2]");                          // each table entry stores three bytes
    emitter.instruction("movzx ecx, BYTE PTR [r11 + rsi]");                   // load the mapped first byte
    emitter.instruction("mov BYTE PTR [r9], cl");                            // write the mapped first byte
    emitter.instruction("inc r9");                                            // advance the destination cursor
    emitter.instruction("movzx ecx, BYTE PTR [r11 + rsi + 1]");               // load the mapped second byte
    emitter.instruction("mov BYTE PTR [r9], cl");                            // write the mapped second byte
    emitter.instruction("inc r9");                                            // advance the destination cursor
    emitter.instruction("movzx ecx, BYTE PTR [r11 + rsi + 2]");               // load the mapped third byte
    emitter.instruction("mov BYTE PTR [r9], cl");                            // write the mapped third byte
    emitter.instruction("inc r9");                                            // advance the destination cursor
    emitter.instruction("jmp __rt_mb_convert_kana_k_utf8_copy_x86");          // continue with the next character
    emitter.label("__rt_mb_convert_kana_k_utf8_copy_ascii_x86");
    emitter.instruction("inc r8");                                            // consume one ASCII byte
    emitter.instruction("dec r10");                                           // account for the consumed byte
    emitter.instruction("mov BYTE PTR [r9], cl");                            // copy the ASCII byte unchanged
    emitter.instruction("inc r9");                                            // advance the destination cursor
    emitter.instruction("jmp __rt_mb_convert_kana_k_utf8_copy_x86");          // continue with the next character
    emitter.label("__rt_mb_convert_kana_k_utf8_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                      // return the owned result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                      // return the unchanged result length
    emitter.instruction("leave");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                               // transfer the owned converted string
    emitter.label("__rt_mb_convert_kana_k_utf8_fallback_x86");
    emitter.instruction("xor eax, eax");                                       // signal that the caller must use the bridge
    emitter.instruction("leave");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                               // return the fallback sentinel
}
