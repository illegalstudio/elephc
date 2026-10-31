//! Purpose:
//! Emits the allocation-light `UTF-8` to `ISO-8859-1` byte converter used by the explicit
//! `mb_convert_encoding()` fast path.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::mbstring::emit_mbstring()`.
//!
//! Key details:
//! - The helper accepts only ASCII bytes and complete two-byte UTF-8 sequences whose decoded
//!   codepoint is at most U+00FF. Anything else returns a null result pointer so the Unicode
//!   bridge keeps PHP's substitution and replacement semantics.
//! - A successful conversion allocates one owned string of at most the source byte length and
//!   returns the actual converted length.
//! - AArch64 input: `x1`/`x2` = source pointer/length; output is the same pair, or `x1 = 0`.
//!   x86_64 input: `rax`/`rdx`; output is the same pair, or `rax = 0`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the UTF-8 to ISO-8859-1 converter for the active target.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits the AArch64 converter.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_utf8_to_latin1 ---");
    emitter.label_global("__rt_mb_utf8_to_latin1");
    emitter.instruction("mov x9, x1");                                          // pre-scan cursor
    emitter.instruction("mov x10, x2");                                        // remaining pre-scan bytes
    emitter.label("__rt_mb_utf8_to_latin1_scan");
    emitter.instruction("cbz x10, __rt_mb_utf8_to_latin1_supported");          // every byte is representable
    emitter.instruction("ldrb w11, [x9]");                                     // load the next source byte
    emitter.instruction("cmp w11, #0x80");                                     // is this a plain ASCII byte?
    emitter.instruction("b.lo __rt_mb_utf8_to_latin1_scan_ascii");             // copy ASCII unchanged
    emitter.instruction("cmp w11, #0xC2");                                     // is this below the Latin-1 two-byte leads?
    emitter.instruction("b.lo __rt_mb_utf8_to_latin1_fallback");               // continuations and overlong leads need the bridge
    emitter.instruction("cmp w11, #0xC3");                                     // is this above the Latin-1 two-byte leads?
    emitter.instruction("b.hi __rt_mb_utf8_to_latin1_fallback");               // three- and four-byte sequences need the bridge
    emitter.instruction("cmp x10, #2");                                        // is a complete two-byte sequence present?
    emitter.instruction("b.lo __rt_mb_utf8_to_latin1_fallback");               // a truncated sequence needs the bridge
    emitter.instruction("ldrb w12, [x9, #1]");                                 // load the second byte
    emitter.instruction("sub w13, w12, #0x80");                                // map the continuation byte to 0..63
    emitter.instruction("cmp w13, #0x3F");                                     // is this a continuation byte?
    emitter.instruction("b.hi __rt_mb_utf8_to_latin1_fallback");              // a non-continuation byte needs the bridge
    emitter.instruction("add x9, x9, #2");                                     // consume the two-byte sequence
    emitter.instruction("sub x10, x10, #2");                                   // account for the consumed bytes
    emitter.instruction("b __rt_mb_utf8_to_latin1_scan");                      // inspect the next byte
    emitter.label("__rt_mb_utf8_to_latin1_scan_ascii");
    emitter.instruction("add x9, x9, #1");                                     // consume one ASCII byte
    emitter.instruction("sub x10, x10, #1");                                   // account for the consumed byte
    emitter.instruction("b __rt_mb_utf8_to_latin1_scan");                      // inspect the next byte
    emitter.label("__rt_mb_utf8_to_latin1_supported");
    emitter.instruction("sub sp, sp, #48");                                    // reserve the source pair, destination, and linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                   // establish a stable frame base
    emitter.instruction("stp x1, x2, [sp, #0]");                               // save the source pointer and length
    emitter.instruction("mov x0, x2");                                         // the converted result never exceeds the source bytes
    emitter.instruction("bl __rt_str_new_owned");                             // allocate one owned result
    emitter.instruction("mov x13, x0");                                        // destination base
    emitter.instruction("mov x9, x0");                                         // destination cursor
    emitter.instruction("ldr x1, [sp, #0]");                                   // reload the source cursor
    emitter.instruction("ldr x10, [sp, #8]");                                  // reload the remaining source bytes
    emitter.label("__rt_mb_utf8_to_latin1_copy");
    emitter.instruction("cbz x10, __rt_mb_utf8_to_latin1_done");               // stop after the final source byte
    emitter.instruction("ldrb w11, [x1], #1");                                 // load and consume one source byte
    emitter.instruction("cmp w11, #0x80");                                     // is this a plain ASCII byte?
    emitter.instruction("b.lo __rt_mb_utf8_to_latin1_store");                  // store ASCII unchanged
    emitter.instruction("ldrb w12, [x1], #1");                                 // load and consume the continuation byte
    emitter.instruction("and w11, w11, #0x1F");                                // keep the lead's five payload bits
    emitter.instruction("lsl w11, w11, #6");                                   // move them above the continuation payload
    emitter.instruction("and w12, w12, #0x3F");                                // keep the continuation's six payload bits
    emitter.instruction("orr w11, w11, w12");                                  // assemble the Latin-1 codepoint
    emitter.instruction("sub x10, x10, #1");                                   // account for the extra source byte
    emitter.label("__rt_mb_utf8_to_latin1_store");
    emitter.instruction("strb w11, [x9], #1");                                 // store one Latin-1 byte
    emitter.instruction("sub x10, x10, #1");                                   // account for the consumed source byte
    emitter.instruction("b __rt_mb_utf8_to_latin1_copy");                      // continue with the next source byte
    emitter.label("__rt_mb_utf8_to_latin1_done");
    emitter.instruction("sub x2, x9, x13");                                    // return the actual converted length
    emitter.instruction("mov x1, x13");                                        // return the owned destination pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                    // release the conversion frame
    emitter.instruction("ret");                                               // transfer the owned Latin-1 string
    emitter.label("__rt_mb_utf8_to_latin1_fallback");
    emitter.instruction("mov x1, xzr");                                        // signal that the caller must use the bridge
    emitter.instruction("ret");                                               // return the fallback sentinel
}

/// Emits the x86_64 converter.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_utf8_to_latin1 ---");
    emitter.label_global("__rt_mb_utf8_to_latin1");
    emitter.instruction("mov r8, rax");                                         // pre-scan cursor
    emitter.instruction("mov r9, rdx");                                        // remaining pre-scan bytes
    emitter.label("__rt_mb_utf8_to_latin1_scan_x86");
    emitter.instruction("test r9, r9");                                        // is the pre-scan complete?
    emitter.instruction("jz __rt_mb_utf8_to_latin1_supported_x86");            // every byte is representable
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                           // load the next source byte
    emitter.instruction("cmp ecx, 0x80");                                      // is this a plain ASCII byte?
    emitter.instruction("jb __rt_mb_utf8_to_latin1_scan_ascii_x86");           // copy ASCII unchanged
    emitter.instruction("cmp ecx, 0xC2");                                      // is this below the Latin-1 two-byte leads?
    emitter.instruction("jb __rt_mb_utf8_to_latin1_fallback_x86");             // continuations and overlong leads need the bridge
    emitter.instruction("cmp ecx, 0xC3");                                      // is this above the Latin-1 two-byte leads?
    emitter.instruction("ja __rt_mb_utf8_to_latin1_fallback_x86");             // three- and four-byte sequences need the bridge
    emitter.instruction("cmp r9, 2");                                          // is a complete two-byte sequence present?
    emitter.instruction("jb __rt_mb_utf8_to_latin1_fallback_x86");             // a truncated sequence needs the bridge
    emitter.instruction("movzx r10d, BYTE PTR [r8 + 1]");                      // load the second byte
    emitter.instruction("sub r10d, 0x80");                                     // map the continuation byte to 0..63
    emitter.instruction("cmp r10d, 0x3F");                                     // is this a continuation byte?
    emitter.instruction("ja __rt_mb_utf8_to_latin1_fallback_x86");            // a non-continuation byte needs the bridge
    emitter.instruction("add r8, 2");                                          // consume the two-byte sequence
    emitter.instruction("sub r9, 2");                                         // account for the consumed bytes
    emitter.instruction("jmp __rt_mb_utf8_to_latin1_scan_x86");                // inspect the next byte
    emitter.label("__rt_mb_utf8_to_latin1_scan_ascii_x86");
    emitter.instruction("inc r8");                                            // consume one ASCII byte
    emitter.instruction("dec r9");                                            // account for the consumed byte
    emitter.instruction("jmp __rt_mb_utf8_to_latin1_scan_x86");                // inspect the next byte
    emitter.label("__rt_mb_utf8_to_latin1_supported_x86");
    emitter.instruction("push rbp");                                          // preserve the caller frame across allocation
    emitter.instruction("mov rbp, rsp");                                      // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                       // reserve the source pair and destination
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                      // save the source pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                     // save the source length
    emitter.instruction("mov rax, rdx");                                      // the converted result never exceeds the source bytes
    emitter.instruction("call __rt_str_new_owned");                           // allocate one owned result
    emitter.instruction("mov r8, rax");                                       // destination base
    emitter.instruction("mov r11, rax");                                      // destination cursor
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                       // reload the source cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                     // reload the remaining source bytes
    emitter.label("__rt_mb_utf8_to_latin1_copy_x86");
    emitter.instruction("test r10, r10");                                     // stop after the final source byte
    emitter.instruction("jz __rt_mb_utf8_to_latin1_done_x86");                // publish the owned result
    emitter.instruction("movzx eax, BYTE PTR [r9]");                          // load one source byte
    emitter.instruction("inc r9");                                            // consume the source byte
    emitter.instruction("cmp eax, 0x80");                                      // is this a plain ASCII byte?
    emitter.instruction("jb __rt_mb_utf8_to_latin1_store_x86");                // store ASCII unchanged
    emitter.instruction("movzx ecx, BYTE PTR [r9]");                          // load the continuation byte
    emitter.instruction("inc r9");                                            // consume the continuation byte
    emitter.instruction("and eax, 0x1F");                                      // keep the lead's five payload bits
    emitter.instruction("shl eax, 6");                                         // move them above the continuation payload
    emitter.instruction("and ecx, 0x3F");                                      // keep the continuation's six payload bits
    emitter.instruction("or eax, ecx");                                       // assemble the Latin-1 codepoint
    emitter.instruction("dec r10");                                           // account for the extra source byte
    emitter.label("__rt_mb_utf8_to_latin1_store_x86");
    emitter.instruction("mov BYTE PTR [r11], al");                            // store one Latin-1 byte
    emitter.instruction("inc r11");                                           // advance the destination cursor
    emitter.instruction("dec r10");                                           // account for the consumed source byte
    emitter.instruction("jmp __rt_mb_utf8_to_latin1_copy_x86");                // continue with the next source byte
    emitter.label("__rt_mb_utf8_to_latin1_done_x86");
    emitter.instruction("mov rdx, r11");                                      // return the actual converted length
    emitter.instruction("sub rdx, r8");                                       // converted length = cursor - base
    emitter.instruction("mov rax, r8");                                       // return the owned destination pointer
    emitter.instruction("leave");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                              // transfer the owned Latin-1 string
    emitter.label("__rt_mb_utf8_to_latin1_fallback_x86");
    emitter.instruction("xor eax, eax");                                      // signal that the caller must use the bridge
    emitter.instruction("ret");                                              // return the fallback sentinel
}
