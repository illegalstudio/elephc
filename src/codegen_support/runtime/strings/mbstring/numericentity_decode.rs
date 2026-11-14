//! Purpose:
//! Emits `__rt_mb_decode_numericentity_utf8`, the owned-string decoder behind the
//! explicit-UTF-8 `mb_decode_numericentity()` fast path with the full decimal Unicode map.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::mbstring::emit_mbstring()`.
//!
//! Key details:
//! - Input is a borrowed slice in `x1`/`x2` (AArch64) or `rax`/`rdx` (x86_64); output is
//!   one owned UTF-8 string in `x1`/`x2` or `rax`/`rdx`, or a null pointer when the shared
//!   decoder-length pre-scan rejects the input and the bridge must run.
//! - The pre-scan (`__rt_mb_decode_numericentity_len`) already validates every reference:
//!   decimal `&#N;`, N in U+0080..U+10FFFF, no surrogates. This pass can therefore parse
//!   the digits without repeating the range checks and encode each scalar directly.
//! - Ordinary bytes copy one for one, matching the length helper's contract.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the UTF-8 numeric-entity decoder for the active target.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits the AArch64 UTF-8 numeric-entity decoder.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_decode_numericentity_utf8 ---");
    emitter.label_global("__rt_mb_decode_numericentity_utf8");
    emitter.instruction("sub sp, sp, #96");                                     // reserve the source and result slots
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #80");                                    // establish a stable frame base
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save the borrowed source slice
    emitter.instruction("bl __rt_mb_decode_numericentity_len");                 // x0 = decoded byte length or -1 for a rejected shape
    emitter.instruction("cmp x0, #-1");                                         // did the shared pre-scan decline the input?
    emitter.instruction("b.eq __rt_mb_decode_numericentity_utf8_fallback");     // rejected shapes keep the bridge's exact semantics
    emitter.instruction("str x0, [sp, #24]");                                   // save the output length across allocation
    emitter.instruction("bl __rt_str_new_owned");                               // allocate exactly the pre-scanned output length
    emitter.instruction("str x0, [sp, #16]");                                   // save the destination base
    emitter.instruction("mov x11, x0");                                         // destination cursor
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source cursor
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the remaining source bytes
    emitter.label("__rt_mb_decode_numericentity_utf8_copy");
    emitter.instruction("cbz x10, __rt_mb_decode_numericentity_utf8_done");     // stop after the final source byte
    emitter.instruction("ldrb w12, [x1]");                                      // load the next source byte
    emitter.instruction("cmp w12, #38");                                        // is this a possible reference opener?
    emitter.instruction("b.ne __rt_mb_decode_numericentity_utf8_plain");        // copy ordinary bytes unchanged
    emitter.instruction("cmp x10, #2");                                         // can the marker byte be read?
    emitter.instruction("b.lo __rt_mb_decode_numericentity_utf8_plain");        // a lone ampersand copies one byte
    emitter.instruction("ldrb w13, [x1, #1]");                                  // load the reference marker
    emitter.instruction("cmp w13, #35");                                        // is this a decimal numeric entity?
    emitter.instruction("b.ne __rt_mb_decode_numericentity_utf8_plain");        // a plain ampersand copies one byte
    emitter.instruction("b __rt_mb_decode_numericentity_utf8_entity");          // parse and encode the reference
    emitter.label("__rt_mb_decode_numericentity_utf8_plain");
    emitter.instruction("strb w12, [x11], #1");                                 // copy the ordinary byte unchanged
    emitter.instruction("add x1, x1, #1");                                      // consume one source byte
    emitter.instruction("sub x10, x10, #1");                                    // account for the consumed byte
    emitter.instruction("b __rt_mb_decode_numericentity_utf8_copy");            // inspect the next source byte
    emitter.label("__rt_mb_decode_numericentity_utf8_entity");
    emitter.instruction("add x1, x1, #2");                                      // skip the validated "&#" opener
    emitter.instruction("sub x10, x10, #2");                                    // account for the consumed opener
    emitter.instruction("mov w14, wzr");                                        // parsed codepoint
    emitter.label("__rt_mb_decode_numericentity_utf8_digits");
    emitter.instruction("ldrb w13, [x1]");                                      // load the next entity byte
    emitter.instruction("sub w15, w13, #48");                                   // map it to 0..9
    emitter.instruction("cmp w15, #9");                                         // is this still a decimal digit?
    emitter.instruction("b.hi __rt_mb_decode_numericentity_utf8_digits_done");  // the digit run ended
    emitter.instruction("mov w16, #10");                                        // decimal radix
    emitter.instruction("madd w14, w14, w16, w15");                             // append the parsed digit
    emitter.instruction("add x1, x1, #1");                                      // consume the digit
    emitter.instruction("sub x10, x10, #1");                                    // account for the consumed digit
    emitter.instruction("b __rt_mb_decode_numericentity_utf8_digits");          // parse the next digit
    emitter.label("__rt_mb_decode_numericentity_utf8_digits_done");
    emitter.instruction("add x1, x1, #1");                                      // skip the validated terminator
    emitter.instruction("sub x10, x10, #1");                                    // account for the consumed terminator
    emitter.instruction("cmp w14, #0x800");                                     // does the codepoint need two UTF-8 bytes?
    emitter.instruction("b.lo __rt_mb_decode_numericentity_utf8_two");          // encode the two-byte sequence
    emitter.instruction("cmp w14, #0x10000");                                   // does the codepoint need three UTF-8 bytes?
    emitter.instruction("b.lo __rt_mb_decode_numericentity_utf8_three");        // encode the three-byte sequence
    emitter.instruction("lsr w15, w14, #18");                                   // high six payload bits of a four-byte head
    emitter.instruction("orr w15, w15, #0xF0");                                 // mark the four-byte lead
    emitter.instruction("strb w15, [x11], #1");                                 // write the four-byte head
    emitter.instruction("lsr w15, w14, #12");                                   // next payload group
    emitter.instruction("and w15, w15, #0x3F");                                 // keep six bits
    emitter.instruction("orr w15, w15, #0x80");                                 // mark a continuation byte
    emitter.instruction("strb w15, [x11], #1");                                 // write the second byte
    emitter.instruction("lsr w15, w14, #6");                                    // next payload group
    emitter.instruction("and w15, w15, #0x3F");                                 // keep six bits
    emitter.instruction("orr w15, w15, #0x80");                                 // mark a continuation byte
    emitter.instruction("strb w15, [x11], #1");                                 // write the third byte
    emitter.instruction("and w15, w14, #0x3F");                                 // low six payload bits
    emitter.instruction("orr w15, w15, #0x80");                                 // mark a continuation byte
    emitter.instruction("strb w15, [x11], #1");                                 // write the fourth byte
    emitter.instruction("b __rt_mb_decode_numericentity_utf8_copy");            // inspect the next source byte
    emitter.label("__rt_mb_decode_numericentity_utf8_three");
    emitter.instruction("lsr w15, w14, #12");                                   // high four payload bits of a three-byte head
    emitter.instruction("orr w15, w15, #0xE0");                                 // mark the three-byte lead
    emitter.instruction("strb w15, [x11], #1");                                 // write the three-byte head
    emitter.instruction("lsr w15, w14, #6");                                    // next payload group
    emitter.instruction("and w15, w15, #0x3F");                                 // keep six bits
    emitter.instruction("orr w15, w15, #0x80");                                 // mark a continuation byte
    emitter.instruction("strb w15, [x11], #1");                                 // write the second byte
    emitter.instruction("and w15, w14, #0x3F");                                 // low six payload bits
    emitter.instruction("orr w15, w15, #0x80");                                 // mark a continuation byte
    emitter.instruction("strb w15, [x11], #1");                                 // write the third byte
    emitter.instruction("b __rt_mb_decode_numericentity_utf8_copy");            // inspect the next source byte
    emitter.label("__rt_mb_decode_numericentity_utf8_two");
    emitter.instruction("lsr w15, w14, #6");                                    // high five payload bits of a two-byte head
    emitter.instruction("orr w15, w15, #0xC0");                                 // mark the two-byte lead
    emitter.instruction("strb w15, [x11], #1");                                 // write the two-byte head
    emitter.instruction("and w15, w14, #0x3F");                                 // low six payload bits
    emitter.instruction("orr w15, w15, #0x80");                                 // mark a continuation byte
    emitter.instruction("strb w15, [x11], #1");                                 // write the second byte
    emitter.instruction("b __rt_mb_decode_numericentity_utf8_copy");            // inspect the next source byte
    emitter.label("__rt_mb_decode_numericentity_utf8_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return the owned result pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return the output byte length
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #96");                                     // release the decoder frame
    emitter.instruction("ret");                                                 // transfer the owned decoded string
    emitter.label("__rt_mb_decode_numericentity_utf8_fallback");
    emitter.instruction("mov x1, xzr");                                         // signal that the caller must use the bridge
    emitter.instruction("mov x2, xzr");                                         // no bytes for a rejected input
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #96");                                     // release the decoder frame
    emitter.instruction("ret");                                                 // return the fallback sentinel
}

/// Emits the x86_64 UTF-8 numeric-entity decoder.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_decode_numericentity_utf8 ---");
    emitter.label_global("__rt_mb_decode_numericentity_utf8");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 96");                                         // reserve the source and result slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the borrowed source pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the borrowed source length
    emitter.instruction("call __rt_mb_decode_numericentity_len");               // rax = decoded byte length or -1 for a rejected shape
    emitter.instruction("cmp rax, -1");                                         // did the shared pre-scan decline the input?
    emitter.instruction("je __rt_mb_decode_numericentity_utf8_fallback_x86");   // rejected shapes keep the bridge's exact semantics
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the output length across allocation
    emitter.instruction("call __rt_str_new_owned");                             // allocate exactly the pre-scanned output length
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the destination base
    emitter.instruction("mov r11, rax");                                        // destination cursor
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the source cursor
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the remaining source bytes
    emitter.label("__rt_mb_decode_numericentity_utf8_copy_x86");
    emitter.instruction("test r9, r9");                                         // is the source exhausted?
    emitter.instruction("jz __rt_mb_decode_numericentity_utf8_done_x86");       // publish the decoded result
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the next source byte
    emitter.instruction("cmp eax, 38");                                         // is this a possible reference opener?
    emitter.instruction("jne __rt_mb_decode_numericentity_utf8_plain_x86");     // copy ordinary bytes unchanged
    emitter.instruction("cmp r9, 2");                                           // can the marker byte be read?
    emitter.instruction("jb __rt_mb_decode_numericentity_utf8_plain_x86");      // a lone ampersand copies one byte
    emitter.instruction("cmp BYTE PTR [r8 + 1], 35");                           // is this a decimal numeric entity?
    emitter.instruction("jne __rt_mb_decode_numericentity_utf8_plain_x86");     // a plain ampersand copies one byte
    emitter.instruction("jmp __rt_mb_decode_numericentity_utf8_entity_x86");    // parse and encode the reference
    emitter.label("__rt_mb_decode_numericentity_utf8_plain_x86");
    emitter.instruction("mov BYTE PTR [r11], al");                              // copy the ordinary byte unchanged
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("inc r8");                                              // consume one source byte
    emitter.instruction("dec r9");                                              // account for the consumed byte
    emitter.instruction("jmp __rt_mb_decode_numericentity_utf8_copy_x86");      // inspect the next source byte
    emitter.label("__rt_mb_decode_numericentity_utf8_entity_x86");
    emitter.instruction("add r8, 2");                                           // skip the validated "&#" opener
    emitter.instruction("sub r9, 2");                                           // account for the consumed opener
    emitter.instruction("xor ecx, ecx");                                        // parsed codepoint
    emitter.label("__rt_mb_decode_numericentity_utf8_digits_x86");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the next entity byte
    emitter.instruction("sub eax, 48");                                         // map it to 0..9
    emitter.instruction("cmp eax, 9");                                          // is this still a decimal digit?
    emitter.instruction("ja __rt_mb_decode_numericentity_utf8_digits_done_x86"); // the digit run ended
    emitter.instruction("imul ecx, ecx, 10");                                   // shift the parsed codepoint
    emitter.instruction("add ecx, eax");                                        // append the parsed digit
    emitter.instruction("inc r8");                                              // consume the digit
    emitter.instruction("dec r9");                                              // account for the consumed digit
    emitter.instruction("jmp __rt_mb_decode_numericentity_utf8_digits_x86");    // parse the next digit
    emitter.label("__rt_mb_decode_numericentity_utf8_digits_done_x86");
    emitter.instruction("inc r8");                                              // skip the validated terminator
    emitter.instruction("dec r9");                                              // account for the consumed terminator
    emitter.instruction("cmp ecx, 0x800");                                      // does the codepoint need two UTF-8 bytes?
    emitter.instruction("jb __rt_mb_decode_numericentity_utf8_two_x86");        // encode the two-byte sequence
    emitter.instruction("cmp ecx, 0x10000");                                    // does the codepoint need three UTF-8 bytes?
    emitter.instruction("jb __rt_mb_decode_numericentity_utf8_three_x86");      // encode the three-byte sequence
    emitter.instruction("mov eax, ecx");                                        // copy the codepoint for the four-byte head
    emitter.instruction("shr eax, 18");                                         // high six payload bits
    emitter.instruction("or eax, 0xF0");                                        // mark the four-byte lead
    emitter.instruction("mov BYTE PTR [r11], al");                              // write the four-byte head
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("mov eax, ecx");                                        // copy the codepoint for the second byte
    emitter.instruction("shr eax, 12");                                         // next payload group
    emitter.instruction("and eax, 0x3F");                                       // keep six bits
    emitter.instruction("or eax, 0x80");                                        // mark a continuation byte
    emitter.instruction("mov BYTE PTR [r11], al");                              // write the second byte
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("mov eax, ecx");                                        // copy the codepoint for the third byte
    emitter.instruction("shr eax, 6");                                          // next payload group
    emitter.instruction("and eax, 0x3F");                                       // keep six bits
    emitter.instruction("or eax, 0x80");                                        // mark a continuation byte
    emitter.instruction("mov BYTE PTR [r11], al");                              // write the third byte
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("mov eax, ecx");                                        // copy the codepoint for the fourth byte
    emitter.instruction("and eax, 0x3F");                                       // low six payload bits
    emitter.instruction("or eax, 0x80");                                        // mark a continuation byte
    emitter.instruction("mov BYTE PTR [r11], al");                              // write the fourth byte
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("jmp __rt_mb_decode_numericentity_utf8_copy_x86");      // inspect the next source byte
    emitter.label("__rt_mb_decode_numericentity_utf8_three_x86");
    emitter.instruction("mov eax, ecx");                                        // copy the codepoint for the three-byte head
    emitter.instruction("shr eax, 12");                                         // high four payload bits
    emitter.instruction("or eax, 0xE0");                                        // mark the three-byte lead
    emitter.instruction("mov BYTE PTR [r11], al");                              // write the three-byte head
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("mov eax, ecx");                                        // copy the codepoint for the second byte
    emitter.instruction("shr eax, 6");                                          // next payload group
    emitter.instruction("and eax, 0x3F");                                       // keep six bits
    emitter.instruction("or eax, 0x80");                                        // mark a continuation byte
    emitter.instruction("mov BYTE PTR [r11], al");                              // write the second byte
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("mov eax, ecx");                                        // copy the codepoint for the third byte
    emitter.instruction("and eax, 0x3F");                                       // low six payload bits
    emitter.instruction("or eax, 0x80");                                        // mark a continuation byte
    emitter.instruction("mov BYTE PTR [r11], al");                              // write the third byte
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("jmp __rt_mb_decode_numericentity_utf8_copy_x86");      // inspect the next source byte
    emitter.label("__rt_mb_decode_numericentity_utf8_two_x86");
    emitter.instruction("mov eax, ecx");                                        // copy the codepoint for the two-byte head
    emitter.instruction("shr eax, 6");                                          // high five payload bits
    emitter.instruction("or eax, 0xC0");                                        // mark the two-byte lead
    emitter.instruction("mov BYTE PTR [r11], al");                              // write the two-byte head
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("mov eax, ecx");                                        // copy the codepoint for the second byte
    emitter.instruction("and eax, 0x3F");                                       // low six payload bits
    emitter.instruction("or eax, 0x80");                                        // mark a continuation byte
    emitter.instruction("mov BYTE PTR [r11], al");                              // write the second byte
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("jmp __rt_mb_decode_numericentity_utf8_copy_x86");      // inspect the next source byte
    emitter.label("__rt_mb_decode_numericentity_utf8_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the owned result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // return the output byte length
    emitter.instruction("leave");                                               // restore the caller frame pointer
    emitter.instruction("ret");                                                 // transfer the owned decoded string
    emitter.label("__rt_mb_decode_numericentity_utf8_fallback_x86");
    emitter.instruction("xor eax, eax");                                        // signal that the caller must use the bridge
    emitter.instruction("xor edx, edx");                                        // no bytes for a rejected input
    emitter.instruction("leave");                                               // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the fallback sentinel
}
