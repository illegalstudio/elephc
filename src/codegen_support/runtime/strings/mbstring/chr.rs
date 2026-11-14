//! Purpose:
//! Emits `__rt_mb_chr_utf8`, the native UTF-8 encoder behind the explicit-UTF-8
//! `mb_chr()` fast path.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::mbstring::emit_mbstring()`.
//!
//! Key details:
//! - Input is the PHP codepoint in `x0`/`rax`; output is an owned string slice in
//!   `x1`/`x2` (AArch64) or `rax`/`rdx` (x86_64). A null pointer reports PHP
//!   `false` for a negative, out-of-range, or surrogate codepoint.
//! - The helper allocates exactly 1-4 bytes and encodes directly into the owned
//!   payload, so the bridge's encoding catalog and result boxing stay out of the
//!   hot path.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the UTF-8 character encoder for the active target.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits the AArch64 UTF-8 character encoder.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_chr_utf8 ---");
    emitter.label_global("__rt_mb_chr_utf8");
    emitter.instruction("cmp x0, #0");                                          // is the codepoint negative?
    emitter.instruction("b.lt __rt_mb_chr_utf8_invalid");                       // negative codepoints return PHP false
    emitter.instruction("mov w9, #0xFFFF");                                     // low half of the last Unicode codepoint
    emitter.instruction("movk w9, #0x10, lsl #16");                             // high half of the last Unicode codepoint
    emitter.instruction("cmp x0, x9");                                          // is the codepoint outside Unicode?
    emitter.instruction("b.hi __rt_mb_chr_utf8_invalid");                       // out-of-range codepoints return false
    emitter.instruction("movz w9, #0xD800");                                    // first UTF-16 surrogate
    emitter.instruction("cmp w0, w9");                                          // is the codepoint below the surrogate range?
    emitter.instruction("b.lo __rt_mb_chr_utf8_valid");                         // ordinary scalars proceed
    emitter.instruction("movz w9, #0xDFFF");                                    // last UTF-16 surrogate
    emitter.instruction("cmp w0, w9");                                          // is the codepoint inside the surrogate range?
    emitter.instruction("b.ls __rt_mb_chr_utf8_invalid");                       // surrogates cannot be encoded
    emitter.label("__rt_mb_chr_utf8_valid");
    emitter.instruction("mov x10, #1");                                         // assume a one-byte ASCII scalar
    emitter.instruction("cmp w0, #0x80");                                       // does the codepoint fit one UTF-8 byte?
    emitter.instruction("b.lo __rt_mb_chr_utf8_len_ready");                     // yes → the length is already one
    emitter.instruction("add x10, x10, #1");                                    // a two-byte sequence
    emitter.instruction("cmp w0, #0x800");                                      // does the codepoint fit two UTF-8 bytes?
    emitter.instruction("b.lo __rt_mb_chr_utf8_len_ready");                     // yes → keep the two-byte length
    emitter.instruction("add x10, x10, #1");                                    // a three-byte sequence
    emitter.instruction("cmp w0, #0x10000");                                    // does the codepoint fit three UTF-8 bytes?
    emitter.instruction("b.lo __rt_mb_chr_utf8_len_ready");                     // yes → keep the three-byte length
    emitter.instruction("add x10, x10, #1");                                    // the remaining scalars need four bytes
    emitter.label("__rt_mb_chr_utf8_len_ready");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the codepoint and length across allocation
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a stable frame base
    emitter.instruction("str w0, [sp, #0]");                                    // save the codepoint
    emitter.instruction("str x10, [sp, #8]");                                   // save the encoded length
    emitter.instruction("mov x0, x10");                                         // allocate exactly the encoded length
    emitter.instruction("bl __rt_str_new_owned");                               // create one owned result string
    emitter.instruction("mov x1, x0");                                          // result pointer
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the encoded length
    emitter.instruction("ldr w9, [sp, #0]");                                    // reload the codepoint
    emitter.instruction("mov x2, x10");                                         // result length
    emitter.instruction("cmp x10, #1");                                         // one-byte encoding?
    emitter.instruction("b.eq __rt_mb_chr_utf8_enc1");                          // write the ASCII byte
    emitter.instruction("cmp x10, #2");                                         // two-byte encoding?
    emitter.instruction("b.eq __rt_mb_chr_utf8_enc2");                          // write the two-byte sequence
    emitter.instruction("cmp x10, #3");                                         // three-byte encoding?
    emitter.instruction("b.eq __rt_mb_chr_utf8_enc3");                          // write the three-byte sequence
    emitter.instruction("lsr w11, w9, #18");                                    // high six payload bits of a four-byte head
    emitter.instruction("orr w11, w11, #0xF0");                                 // mark the four-byte lead
    emitter.instruction("strb w11, [x1]");                                      // write the four-byte head
    emitter.instruction("lsr w11, w9, #12");                                    // next payload group
    emitter.instruction("and w11, w11, #0x3F");                                 // keep six bits
    emitter.instruction("orr w11, w11, #0x80");                                 // mark a continuation byte
    emitter.instruction("strb w11, [x1, #1]");                                  // write the second byte
    emitter.instruction("lsr w11, w9, #6");                                     // next payload group
    emitter.instruction("and w11, w11, #0x3F");                                 // keep six bits
    emitter.instruction("orr w11, w11, #0x80");                                 // mark a continuation byte
    emitter.instruction("strb w11, [x1, #2]");                                  // write the third byte
    emitter.instruction("and w11, w9, #0x3F");                                  // low six payload bits
    emitter.instruction("orr w11, w11, #0x80");                                 // mark a continuation byte
    emitter.instruction("strb w11, [x1, #3]");                                  // write the fourth byte
    emitter.instruction("b __rt_mb_chr_utf8_done");                             // publish the encoded slice
    emitter.label("__rt_mb_chr_utf8_enc3");
    emitter.instruction("lsr w11, w9, #12");                                    // high four payload bits of a three-byte head
    emitter.instruction("orr w11, w11, #0xE0");                                 // mark the three-byte lead
    emitter.instruction("strb w11, [x1]");                                      // write the three-byte head
    emitter.instruction("lsr w11, w9, #6");                                     // next payload group
    emitter.instruction("and w11, w11, #0x3F");                                 // keep six bits
    emitter.instruction("orr w11, w11, #0x80");                                 // mark a continuation byte
    emitter.instruction("strb w11, [x1, #1]");                                  // write the second byte
    emitter.instruction("and w11, w9, #0x3F");                                  // low six payload bits
    emitter.instruction("orr w11, w11, #0x80");                                 // mark a continuation byte
    emitter.instruction("strb w11, [x1, #2]");                                  // write the third byte
    emitter.instruction("b __rt_mb_chr_utf8_done");                             // publish the encoded slice
    emitter.label("__rt_mb_chr_utf8_enc2");
    emitter.instruction("lsr w11, w9, #6");                                     // high five payload bits of a two-byte head
    emitter.instruction("orr w11, w11, #0xC0");                                 // mark the two-byte lead
    emitter.instruction("strb w11, [x1]");                                      // write the two-byte head
    emitter.instruction("and w11, w9, #0x3F");                                  // low six payload bits
    emitter.instruction("orr w11, w11, #0x80");                                 // mark a continuation byte
    emitter.instruction("strb w11, [x1, #1]");                                  // write the second byte
    emitter.instruction("b __rt_mb_chr_utf8_done");                             // publish the encoded slice
    emitter.label("__rt_mb_chr_utf8_enc1");
    emitter.instruction("strb w9, [x1]");                                       // write the ASCII byte
    emitter.label("__rt_mb_chr_utf8_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                     // release the encoder frame
    emitter.instruction("ret");                                                 // return the owned UTF-8 slice
    emitter.label("__rt_mb_chr_utf8_invalid");
    emitter.instruction("mov x1, xzr");                                         // null pointer reports PHP false
    emitter.instruction("mov x2, xzr");                                         // no bytes for an invalid codepoint
    emitter.instruction("ret");                                                 // return the false sentinel
}

/// Emits the x86_64 UTF-8 character encoder.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_chr_utf8 ---");
    emitter.label_global("__rt_mb_chr_utf8");
    emitter.instruction("test rax, rax");                                       // is the codepoint negative?
    emitter.instruction("js __rt_mb_chr_utf8_invalid_x86");                     // negative codepoints return PHP false
    emitter.instruction("mov r11, 0x10FFFF");                                   // largest Unicode scalar
    emitter.instruction("cmp rax, r11");                                        // is the codepoint outside Unicode?
    emitter.instruction("ja __rt_mb_chr_utf8_invalid_x86");                     // out-of-range codepoints return false
    emitter.instruction("cmp rax, 0xD800");                                     // is the codepoint below the surrogate range?
    emitter.instruction("jb __rt_mb_chr_utf8_valid_x86");                       // ordinary scalars proceed
    emitter.instruction("cmp rax, 0xDFFF");                                     // is the codepoint inside the surrogate range?
    emitter.instruction("jbe __rt_mb_chr_utf8_invalid_x86");                    // surrogates cannot be encoded
    emitter.label("__rt_mb_chr_utf8_valid_x86");
    emitter.instruction("mov r10, 1");                                          // assume a one-byte ASCII scalar
    emitter.instruction("cmp rax, 0x80");                                       // does the codepoint fit one UTF-8 byte?
    emitter.instruction("jb __rt_mb_chr_utf8_len_ready_x86");                   // yes → the length is already one
    emitter.instruction("inc r10");                                             // a two-byte sequence
    emitter.instruction("cmp rax, 0x800");                                      // does the codepoint fit two UTF-8 bytes?
    emitter.instruction("jb __rt_mb_chr_utf8_len_ready_x86");                   // yes → keep the two-byte length
    emitter.instruction("inc r10");                                             // a three-byte sequence
    emitter.instruction("cmp rax, 0x10000");                                    // does the codepoint fit three UTF-8 bytes?
    emitter.instruction("jb __rt_mb_chr_utf8_len_ready_x86");                   // yes → keep the three-byte length
    emitter.instruction("inc r10");                                             // the remaining scalars need four bytes
    emitter.label("__rt_mb_chr_utf8_len_ready_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve the codepoint, length, and pointer slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the codepoint
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the encoded length
    emitter.instruction("mov rax, r10");                                        // allocate exactly the encoded length
    emitter.instruction("call __rt_str_new_owned");                             // create one owned result string
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the codepoint
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the encoded length
    emitter.instruction("cmp r10, 1");                                          // one-byte encoding?
    emitter.instruction("je __rt_mb_chr_utf8_enc1_x86");                        // write the ASCII byte
    emitter.instruction("cmp r10, 2");                                          // two-byte encoding?
    emitter.instruction("je __rt_mb_chr_utf8_enc2_x86");                        // write the two-byte sequence
    emitter.instruction("cmp r10, 3");                                          // three-byte encoding?
    emitter.instruction("je __rt_mb_chr_utf8_enc3_x86");                        // write the three-byte sequence
    emitter.instruction("mov r11, r9");                                         // copy the codepoint for the four-byte head
    emitter.instruction("shr r11, 18");                                         // high six payload bits
    emitter.instruction("or r11, 0xF0");                                        // mark the four-byte lead
    emitter.instruction("mov BYTE PTR [rax], r11b");                            // write the four-byte head
    emitter.instruction("mov r11, r9");                                         // copy the codepoint for the second byte
    emitter.instruction("shr r11, 12");                                         // next payload group
    emitter.instruction("and r11, 0x3F");                                       // keep six bits
    emitter.instruction("or r11, 0x80");                                        // mark a continuation byte
    emitter.instruction("mov BYTE PTR [rax + 1], r11b");                        // write the second byte
    emitter.instruction("mov r11, r9");                                         // copy the codepoint for the third byte
    emitter.instruction("shr r11, 6");                                          // next payload group
    emitter.instruction("and r11, 0x3F");                                       // keep six bits
    emitter.instruction("or r11, 0x80");                                        // mark a continuation byte
    emitter.instruction("mov BYTE PTR [rax + 2], r11b");                        // write the third byte
    emitter.instruction("mov r11, r9");                                         // copy the codepoint for the final byte
    emitter.instruction("and r11, 0x3F");                                       // low six payload bits
    emitter.instruction("or r11, 0x80");                                        // mark a continuation byte
    emitter.instruction("mov BYTE PTR [rax + 3], r11b");                        // write the fourth byte
    emitter.instruction("jmp __rt_mb_chr_utf8_done_x86");                       // publish the encoded slice
    emitter.label("__rt_mb_chr_utf8_enc3_x86");
    emitter.instruction("mov r11, r9");                                         // copy the codepoint for the three-byte head
    emitter.instruction("shr r11, 12");                                         // high four payload bits
    emitter.instruction("or r11, 0xE0");                                        // mark the three-byte lead
    emitter.instruction("mov BYTE PTR [rax], r11b");                            // write the three-byte head
    emitter.instruction("mov r11, r9");                                         // copy the codepoint for the second byte
    emitter.instruction("shr r11, 6");                                          // next payload group
    emitter.instruction("and r11, 0x3F");                                       // keep six bits
    emitter.instruction("or r11, 0x80");                                        // mark a continuation byte
    emitter.instruction("mov BYTE PTR [rax + 1], r11b");                        // write the second byte
    emitter.instruction("mov r11, r9");                                         // copy the codepoint for the third byte
    emitter.instruction("and r11, 0x3F");                                       // low six payload bits
    emitter.instruction("or r11, 0x80");                                        // mark a continuation byte
    emitter.instruction("mov BYTE PTR [rax + 2], r11b");                        // write the third byte
    emitter.instruction("jmp __rt_mb_chr_utf8_done_x86");                       // publish the encoded slice
    emitter.label("__rt_mb_chr_utf8_enc2_x86");
    emitter.instruction("mov r11, r9");                                         // copy the codepoint for the two-byte head
    emitter.instruction("shr r11, 6");                                          // high five payload bits
    emitter.instruction("or r11, 0xC0");                                        // mark the two-byte lead
    emitter.instruction("mov BYTE PTR [rax], r11b");                            // write the two-byte head
    emitter.instruction("mov r11, r9");                                         // copy the codepoint for the second byte
    emitter.instruction("and r11, 0x3F");                                       // low six payload bits
    emitter.instruction("or r11, 0x80");                                        // mark a continuation byte
    emitter.instruction("mov BYTE PTR [rax + 1], r11b");                        // write the second byte
    emitter.instruction("jmp __rt_mb_chr_utf8_done_x86");                       // publish the encoded slice
    emitter.label("__rt_mb_chr_utf8_enc1_x86");
    emitter.instruction("mov BYTE PTR [rax], r9b");                             // write the ASCII byte
    emitter.label("__rt_mb_chr_utf8_done_x86");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the encoded length
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the owned string pointer
    emitter.instruction("leave");                                               // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned UTF-8 slice
    emitter.label("__rt_mb_chr_utf8_invalid_x86");
    emitter.instruction("xor eax, eax");                                        // null pointer reports PHP false
    emitter.instruction("xor edx, edx");                                        // no bytes for an invalid codepoint
    emitter.instruction("ret");                                                 // return the false sentinel
}
