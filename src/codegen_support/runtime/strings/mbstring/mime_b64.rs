//! Purpose:
//! Emits the allocation-light UTF-8 MIME base64 encoder used by the explicit
//! `mb_encode_mimeheader()` fast path.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::mbstring::emit_mbstring()`.
//!
//! Key details:
//! - The helper accepts a non-empty UTF-8 subject of at most 48 bytes whose first byte is
//!   non-ASCII, matching PHP's decision to encode the whole value as one RFC 2047 word.
//! - Anything else returns a null pointer so the shared bridge keeps PHP's chunking,
//!   charset, transfer-encoding, newline, and indent behavior.
//! - AArch64 input is `x1`/`x2` = pointer/length; x86_64 input is `rax`/`rdx`. A successful
//!   call returns an owned string pair with no INI identity metadata.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the MIME base64 wrapper for the active target.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits the AArch64 MIME base64 wrapper.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_mime_b64_utf8 ---");
    emitter.label_global("__rt_mb_mime_b64_utf8");
    emitter.instruction("sub sp, sp, #64");                                     // reserve the source pair, lengths, and linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #48");                                   // establish a stable frame base
    emitter.instruction("stp x1, x2, [sp, #0]");                               // save the source pointer and length
    emitter.instruction("cbz x2, __rt_mb_mime_b64_utf8_fallback");             // an empty subject stays on the bridge
    emitter.instruction("mov x9, x1");                                         // scan through a cursor
    emitter.instruction("mov x10, x2");                                       // bound every access by the source length
    emitter.instruction("mov w12, #0");                                       // no non-ASCII byte seen yet
    emitter.instruction("mov w13, #0");                                       // no ASCII whitespace seen yet
    emitter.label("__rt_mb_mime_b64_utf8_scan");
    emitter.instruction("cbz x10, __rt_mb_mime_b64_utf8_scan_done");           // stop after the final source byte
    emitter.instruction("ldrb w11, [x9], #1");                                 // load and consume one source byte
    emitter.instruction("sub x10, x10, #1");                                   // account for the consumed byte
    emitter.instruction("cmp w11, #0x80");                                     // is this a non-ASCII byte?
    emitter.instruction("b.hs __rt_mb_mime_b64_utf8_non_ascii");               // record the first encoded byte
    emitter.instruction("cmp w11, #0x20");                                     // is this an ASCII space?
    emitter.instruction("b.eq __rt_mb_mime_b64_utf8_whitespace");              // PHP folds text after an ASCII prefix
    emitter.instruction("cmp w11, #0x09");                                     // is this an ASCII tab?
    emitter.instruction("b.eq __rt_mb_mime_b64_utf8_whitespace");              // PHP folds text after an ASCII prefix
    emitter.instruction("cmp w11, #0x0A");                                     // is this an ASCII line feed?
    emitter.instruction("b.eq __rt_mb_mime_b64_utf8_whitespace");              // PHP folds text after an ASCII prefix
    emitter.instruction("cmp w11, #0x0D");                                     // is this an ASCII carriage return?
    emitter.instruction("b.eq __rt_mb_mime_b64_utf8_whitespace");              // PHP folds text after an ASCII prefix
    emitter.instruction("b __rt_mb_mime_b64_utf8_scan");                       // inspect the next source byte
    emitter.label("__rt_mb_mime_b64_utf8_non_ascii");
    emitter.instruction("mov w12, #1");                                        // remember that encoding is required
    emitter.instruction("b __rt_mb_mime_b64_utf8_scan");                       // inspect the next source byte
    emitter.label("__rt_mb_mime_b64_utf8_whitespace");
    emitter.instruction("mov w13, #1");                                        // remember the ASCII prefix boundary
    emitter.instruction("b __rt_mb_mime_b64_utf8_scan");                       // inspect the next source byte
    emitter.label("__rt_mb_mime_b64_utf8_scan_done");
    emitter.instruction("cbnz w12, __rt_mb_mime_b64_utf8_encode");             // a non-ASCII byte requires an encoded word
    emitter.instruction("ldp x1, x2, [sp, #0]");                               // restore the plain ASCII subject
    emitter.instruction("bl __rt_str_dup_owned");                             // return the unchanged value as an owned string
    emitter.instruction("b __rt_mb_mime_b64_utf8_return");                     // skip the encoded-word path
    emitter.label("__rt_mb_mime_b64_utf8_encode");
    emitter.instruction("cbnz w13, __rt_mb_mime_b64_utf8_fallback");           // an ASCII prefix keeps the bridge's chunking
    emitter.instruction("ldr x2, [sp, #8]");                                   // reload the source length
    emitter.instruction("cmp x2, #48");                                        // does the encoded word fit PHP's line limit?
    emitter.instruction("b.hi __rt_mb_mime_b64_utf8_fallback");                // longer values need the folding bridge
    emitter.instruction("ldr x1, [sp, #0]");                                   // reload the source pointer
    emitter.instruction("add x9, x2, #2");                                     // round the source length up to a base64 triplet
    emitter.instruction("mov x10, #3");                                        // base64 encodes three bytes per quartet
    emitter.instruction("udiv x9, x9, x10");                                   // count the encoded quartets
    emitter.instruction("lsl x9, x9, #2");                                     // each quartet contributes four characters
    emitter.instruction("str x9, [sp, #16]");                                  // save the base64 payload length
    emitter.instruction("add x0, x9, #12");                                    // add the 10-byte prefix and 2-byte suffix
    emitter.instruction("bl __rt_str_new_owned");                             // allocate one owned encoded word
    emitter.instruction("str x0, [sp, #24]");                                  // save the destination base
    emitter.instruction("mov x10, x0");                                        // destination cursor for the prefix
    emitter.instruction("mov w11, #0x3D");                                     // '='
    emitter.instruction("strb w11, [x10]");                                    // write the encoded-word opener
    emitter.instruction("mov w11, #0x3F");                                     // '?'
    emitter.instruction("strb w11, [x10, #1]");                                // write the charset separator
    emitter.instruction("mov w11, #0x55");                                     // 'U'
    emitter.instruction("strb w11, [x10, #2]");                                // write the charset name
    emitter.instruction("mov w11, #0x54");                                     // 'T'
    emitter.instruction("strb w11, [x10, #3]");                                // write the charset name
    emitter.instruction("mov w11, #0x46");                                     // 'F'
    emitter.instruction("strb w11, [x10, #4]");                                // write the charset name
    emitter.instruction("mov w11, #0x2D");                                     // '-'
    emitter.instruction("strb w11, [x10, #5]");                                // write the charset suffix
    emitter.instruction("mov w11, #0x38");                                     // '8'
    emitter.instruction("strb w11, [x10, #6]");                                // write the charset version
    emitter.instruction("mov w11, #0x3F");                                     // '?'
    emitter.instruction("strb w11, [x10, #7]");                                // write the transfer-encoding separator
    emitter.instruction("mov w11, #0x42");                                     // 'B'
    emitter.instruction("strb w11, [x10, #8]");                                // select base64 transfer encoding
    emitter.instruction("mov w11, #0x3F");                                     // '?'
    emitter.instruction("strb w11, [x10, #9]");                                // close the encoded-word opener
    emitter.instruction("ldp x1, x2, [sp, #0]");                               // restore the source pointer and length
    emitter.instruction("bl __rt_base64_encode");                             // encode the subject into the concat buffer
    emitter.instruction("str x1, [sp, #32]");                                  // save the base64 source pointer
    emitter.instruction("str x2, [sp, #40]");                                  // save the base64 payload length
    emitter.instruction("ldr x0, [sp, #24]");                                  // load the destination base
    emitter.instruction("add x0, x0, #10");                                    // skip the encoded-word prefix
    emitter.instruction("ldr x1, [sp, #32]");                                  // pass the base64 source pointer
    emitter.instruction("ldr x2, [sp, #40]");                                  // pass the base64 payload length
    emitter.bl_c("memcpy");                                                   // copy the encoded payload behind the prefix
    emitter.instruction("ldr x9, [sp, #24]");                                  // reload the destination base
    emitter.instruction("ldr x10, [sp, #16]");                                 // reload the base64 payload length
    emitter.instruction("add x9, x9, #10");                                    // skip the encoded-word prefix
    emitter.instruction("add x9, x9, x10");                                    // advance past the encoded payload
    emitter.instruction("mov w11, #0x3F");                                     // '?'
    emitter.instruction("strb w11, [x9]");                                     // write the encoded-word closer
    emitter.instruction("mov w11, #0x3D");                                     // '='
    emitter.instruction("strb w11, [x9, #1]");                                 // finish the encoded word
    emitter.instruction("ldr x1, [sp, #24]");                                  // return the owned encoded-word pointer
    emitter.instruction("add x2, x10, #12");                                   // return prefix + payload + suffix
    emitter.label("__rt_mb_mime_b64_utf8_return");
    emitter.instruction("ldp x29, x30, [sp, #48]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                    // release the wrapper frame
    emitter.instruction("ret");                                               // transfer the owned result
    emitter.label("__rt_mb_mime_b64_utf8_fallback");
    emitter.instruction("mov x1, xzr");                                        // signal that the caller must use the bridge
    emitter.instruction("ldp x29, x30, [sp, #48]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                    // release the wrapper frame
    emitter.instruction("ret");                                               // return the fallback sentinel
}

/// Emits the x86_64 MIME base64 wrapper.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_mime_b64_utf8 ---");
    emitter.label_global("__rt_mb_mime_b64_utf8");
    emitter.instruction("push rbp");                                           // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                       // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                        // reserve the source pair, lengths, and linkage
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                       // save the source pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                      // save the source length
    emitter.instruction("test rdx, rdx");                                      // is the subject empty?
    emitter.instruction("jz __rt_mb_mime_b64_utf8_fallback_x86");              // an empty subject stays on the bridge
    emitter.instruction("mov r8, rax");                                       // scan through a cursor
    emitter.instruction("mov r9, rdx");                                       // bound every access by the source length
    emitter.instruction("xor r10d, r10d");                                    // no non-ASCII byte seen yet
    emitter.instruction("xor r11d, r11d");                                    // no ASCII whitespace seen yet
    emitter.label("__rt_mb_mime_b64_utf8_scan_x86");
    emitter.instruction("test r9, r9");                                        // is the source exhausted?
    emitter.instruction("jz __rt_mb_mime_b64_utf8_scan_done_x86");             // finish the prefix decision
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                          // load the next source byte
    emitter.instruction("inc r8");                                            // consume the source byte
    emitter.instruction("dec r9");                                            // account for the consumed byte
    emitter.instruction("cmp ecx, 0x80");                                      // is this a non-ASCII byte?
    emitter.instruction("jae __rt_mb_mime_b64_utf8_non_ascii_x86");            // record the first encoded byte
    emitter.instruction("cmp ecx, 0x20");                                      // is this an ASCII space?
    emitter.instruction("je __rt_mb_mime_b64_utf8_whitespace_x86");            // PHP folds text after an ASCII prefix
    emitter.instruction("cmp ecx, 0x09");                                      // is this an ASCII tab?
    emitter.instruction("je __rt_mb_mime_b64_utf8_whitespace_x86");            // PHP folds text after an ASCII prefix
    emitter.instruction("cmp ecx, 0x0A");                                      // is this an ASCII line feed?
    emitter.instruction("je __rt_mb_mime_b64_utf8_whitespace_x86");            // PHP folds text after an ASCII prefix
    emitter.instruction("cmp ecx, 0x0D");                                      // is this an ASCII carriage return?
    emitter.instruction("je __rt_mb_mime_b64_utf8_whitespace_x86");            // PHP folds text after an ASCII prefix
    emitter.instruction("jmp __rt_mb_mime_b64_utf8_scan_x86");                 // inspect the next source byte
    emitter.label("__rt_mb_mime_b64_utf8_non_ascii_x86");
    emitter.instruction("mov r10d, 1");                                        // remember that encoding is required
    emitter.instruction("jmp __rt_mb_mime_b64_utf8_scan_x86");                 // inspect the next source byte
    emitter.label("__rt_mb_mime_b64_utf8_whitespace_x86");
    emitter.instruction("mov r11d, 1");                                        // remember the ASCII prefix boundary
    emitter.instruction("jmp __rt_mb_mime_b64_utf8_scan_x86");                 // inspect the next source byte
    emitter.label("__rt_mb_mime_b64_utf8_scan_done_x86");
    emitter.instruction("test r10d, r10d");                                    // does the value require encoding?
    emitter.instruction("jnz __rt_mb_mime_b64_utf8_encode_x86");               // encode the whole non-ASCII value
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                       // restore the plain ASCII subject pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                      // restore the plain ASCII subject length
    emitter.instruction("call __rt_str_dup_owned");                          // return the unchanged value as an owned string
    emitter.instruction("jmp __rt_mb_mime_b64_utf8_return_x86");                // skip the encoded-word path
    emitter.label("__rt_mb_mime_b64_utf8_encode_x86");
    emitter.instruction("test r11d, r11d");                                    // was there an ASCII prefix before the non-ASCII byte?
    emitter.instruction("jnz __rt_mb_mime_b64_utf8_fallback_x86");             // an ASCII prefix keeps the bridge's chunking
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                      // reload the source length
    emitter.instruction("cmp rdx, 48");                                        // does the encoded word fit PHP's line limit?
    emitter.instruction("ja __rt_mb_mime_b64_utf8_fallback_x86");              // longer values need the folding bridge
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                       // reload the source pointer
    emitter.instruction("lea rax, [rdx + 2]");                                 // round the source length up to a base64 triplet
    emitter.instruction("xor edx, edx");                                      // clear the dividend high half
    emitter.instruction("mov ecx, 3");                                        // base64 encodes three bytes per quartet
    emitter.instruction("div rcx");                                           // count the encoded quartets
    emitter.instruction("shl rax, 2");                                        // each quartet contributes four characters
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                      // save the base64 payload length
    emitter.instruction("add rax, 12");                                       // add the 10-byte prefix and 2-byte suffix
    emitter.instruction("call __rt_str_new_owned");                           // allocate one owned encoded word
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                      // save the destination base
    emitter.instruction("mov r10, rax");                                      // destination cursor for the prefix
    emitter.instruction("mov BYTE PTR [r10], 0x3D");                          // write the encoded-word opener
    emitter.instruction("mov BYTE PTR [r10 + 1], 0x3F");                      // write the charset separator
    emitter.instruction("mov BYTE PTR [r10 + 2], 0x55");                      // write the charset name
    emitter.instruction("mov BYTE PTR [r10 + 3], 0x54");                      // write the charset name
    emitter.instruction("mov BYTE PTR [r10 + 4], 0x46");                      // write the charset name
    emitter.instruction("mov BYTE PTR [r10 + 5], 0x2D");                      // write the charset suffix
    emitter.instruction("mov BYTE PTR [r10 + 6], 0x38");                      // write the charset version
    emitter.instruction("mov BYTE PTR [r10 + 7], 0x3F");                      // write the transfer-encoding separator
    emitter.instruction("mov BYTE PTR [r10 + 8], 0x42");                      // select base64 transfer encoding
    emitter.instruction("mov BYTE PTR [r10 + 9], 0x3F");                      // close the encoded-word opener
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                       // restore the source pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                      // restore the source length
    emitter.instruction("call __rt_base64_encode");                          // encode the subject into the concat buffer
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                      // save the base64 source pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                      // save the base64 payload length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                      // load the destination base
    emitter.instruction("add rdi, 10");                                       // skip the encoded-word prefix
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                      // pass the base64 source pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                      // pass the base64 payload length
    emitter.bl_c("memcpy");                                                   // copy the encoded payload behind the prefix
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                      // reload the destination base
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                      // reload the base64 payload length
    emitter.instruction("add rcx, 10");                                       // skip the encoded-word prefix
    emitter.instruction("mov BYTE PTR [rax + rcx], 0x3F");                    // write the encoded-word closer
    emitter.instruction("mov BYTE PTR [rax + rcx + 1], 0x3D");                // finish the encoded word
    emitter.instruction("add rcx, 2");                                        // include both suffix bytes in the length
    emitter.instruction("mov rdx, rcx");                                      // return the complete encoded-word length
    emitter.label("__rt_mb_mime_b64_utf8_return_x86");
    emitter.instruction("leave");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                               // transfer the owned result
    emitter.label("__rt_mb_mime_b64_utf8_fallback_x86");
    emitter.instruction("xor eax, eax");                                       // signal that the caller must use the bridge
    emitter.instruction("leave");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                               // return the fallback sentinel
}
