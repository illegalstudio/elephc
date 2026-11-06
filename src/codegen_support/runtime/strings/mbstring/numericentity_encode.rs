//! Purpose:
//! Emits `__rt_mb_encode_numericentity_utf8`, the owned-string encoder behind the
//! explicit-UTF-8 `mb_encode_numericentity()` fast path with the full decimal Unicode map.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::mbstring::emit_mbstring()`.
//!
//! Key details:
//! - Input is a borrowed UTF-8 slice in `x1`/`x2` (AArch64) or `rax`/`rdx` (x86_64);
//!   output is one owned string in `x1`/`x2` or `rax`/`rdx`, or a null pointer when the
//!   bytes are not valid UTF-8 and the shared bridge must reproduce PHP's substitution.
//! - The exact output length comes from `__rt_mb_encode_numericentity_len`, so the
//!   pre-scan validation is shared with the allocation-free length sink.
//! - ASCII copies one byte; every non-ASCII scalar becomes `&#DECIMAL;` with the
//!   codepoint unchanged by the identity offset and full-range mask.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the UTF-8 numeric-entity encoder for the active target.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits the AArch64 UTF-8 numeric-entity encoder.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_encode_numericentity_utf8 ---");
    emitter.label_global("__rt_mb_encode_numericentity_utf8");
    emitter.instruction("sub sp, sp, #112");                                    // reserve the source, result, and digit scratch slots
    emitter.instruction("stp x29, x30, [sp, #96]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #96");                                    // establish a stable frame base
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save the borrowed source slice
    emitter.instruction("bl __rt_mb_encode_numericentity_len");                 // x0 = exact encoded length or -1 for invalid UTF-8
    emitter.instruction("cmp x0, #-1");                                         // did the shared pre-scan decline the input?
    emitter.instruction("b.eq __rt_mb_encode_numericentity_utf8_fallback");     // invalid bytes keep the bridge's substitution behaviour
    emitter.instruction("str x0, [sp, #24]");                                   // save the output length across allocation
    emitter.instruction("bl __rt_str_new_owned");                               // allocate exactly the pre-scanned output length
    emitter.instruction("str x0, [sp, #16]");                                   // save the destination base
    emitter.instruction("mov x11, x0");                                         // destination cursor
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source cursor
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the remaining source bytes
    emitter.label("__rt_mb_encode_numericentity_utf8_copy");
    emitter.instruction("cbz x10, __rt_mb_encode_numericentity_utf8_done");     // stop after the final source byte
    emitter.instruction("ldrb w12, [x1]");                                      // load the next source byte
    emitter.instruction("cmp w12, #0x80");                                      // is this a plain ASCII byte?
    emitter.instruction("b.lo __rt_mb_encode_numericentity_utf8_ascii");        // copy ASCII unchanged
    emitter.instruction("cmp w12, #0xE0");                                      // is this a two-byte lead?
    emitter.instruction("b.lo __rt_mb_encode_numericentity_utf8_two");          // decode the two-byte sequence
    emitter.instruction("cmp w12, #0xF0");                                      // is this a three-byte lead?
    emitter.instruction("b.lo __rt_mb_encode_numericentity_utf8_three");        // decode the three-byte sequence
    emitter.instruction("and w14, w12, #0x07");                                 // keep the four-byte payload bits
    emitter.instruction("ldrb w13, [x1, #1]");                                  // load the first continuation byte
    emitter.instruction("and w13, w13, #0x3F");                                 // keep its payload
    emitter.instruction("lsl w14, w14, #6");                                    // shift the accumulated payload
    emitter.instruction("orr w14, w14, w13");                                   // append the first continuation payload
    emitter.instruction("ldrb w13, [x1, #2]");                                  // load the second continuation byte
    emitter.instruction("and w13, w13, #0x3F");                                 // keep its payload
    emitter.instruction("lsl w14, w14, #6");                                    // shift the accumulated payload
    emitter.instruction("orr w14, w14, w13");                                   // append the second continuation payload
    emitter.instruction("ldrb w13, [x1, #3]");                                  // load the third continuation byte
    emitter.instruction("and w13, w13, #0x3F");                                 // keep its payload
    emitter.instruction("lsl w14, w14, #6");                                    // shift the accumulated payload
    emitter.instruction("orr w14, w14, w13");                                   // append the third continuation payload
    emitter.instruction("add x1, x1, #4");                                      // consume the four-byte sequence
    emitter.instruction("sub x10, x10, #4");                                    // account for the consumed bytes
    emitter.instruction("b __rt_mb_encode_numericentity_utf8_entity");          // emit the numeric reference
    emitter.label("__rt_mb_encode_numericentity_utf8_three");
    emitter.instruction("and w14, w12, #0x0F");                                 // keep the three-byte payload bits
    emitter.instruction("ldrb w13, [x1, #1]");                                  // load the first continuation byte
    emitter.instruction("and w13, w13, #0x3F");                                 // keep its payload
    emitter.instruction("lsl w14, w14, #6");                                    // shift the accumulated payload
    emitter.instruction("orr w14, w14, w13");                                   // append the first continuation payload
    emitter.instruction("ldrb w13, [x1, #2]");                                  // load the second continuation byte
    emitter.instruction("and w13, w13, #0x3F");                                 // keep its payload
    emitter.instruction("lsl w14, w14, #6");                                    // shift the accumulated payload
    emitter.instruction("orr w14, w14, w13");                                   // append the second continuation payload
    emitter.instruction("add x1, x1, #3");                                      // consume the three-byte sequence
    emitter.instruction("sub x10, x10, #3");                                    // account for the consumed bytes
    emitter.instruction("b __rt_mb_encode_numericentity_utf8_entity");          // emit the numeric reference
    emitter.label("__rt_mb_encode_numericentity_utf8_two");
    emitter.instruction("and w14, w12, #0x1F");                                 // keep the two-byte payload bits
    emitter.instruction("ldrb w13, [x1, #1]");                                  // load the continuation byte
    emitter.instruction("and w13, w13, #0x3F");                                 // keep its payload
    emitter.instruction("lsl w14, w14, #6");                                    // shift the accumulated payload
    emitter.instruction("orr w14, w14, w13");                                   // append the continuation payload
    emitter.instruction("add x1, x1, #2");                                      // consume the two-byte sequence
    emitter.instruction("sub x10, x10, #2");                                    // account for the consumed bytes
    emitter.label("__rt_mb_encode_numericentity_utf8_entity");
    emitter.instruction("mov w15, #38");                                        // ASCII '&'
    emitter.instruction("strb w15, [x11], #1");                                 // write the reference opener
    emitter.instruction("mov w15, #35");                                        // ASCII '#'
    emitter.instruction("strb w15, [x11], #1");                                 // write the decimal marker
    emitter.instruction("add x12, sp, #88");                                    // end of the 24-byte digit scratch
    emitter.instruction("mov x15, x12");                                        // digits are built backwards from the end
    emitter.instruction("mov w17, w14");                                        // decimal value to render
    emitter.instruction("mov w16, #10");                                        // decimal radix
    emitter.label("__rt_mb_encode_numericentity_utf8_digits");
    emitter.instruction("udiv w18, w17, w16");                                  // drop the least significant digit
    emitter.instruction("msub w19, w18, w16, w17");                             // recover the removed digit
    emitter.instruction("add w19, w19, #48");                                   // convert the digit to ASCII
    emitter.instruction("sub x15, x15, #1");                                    // move the digit cursor down
    emitter.instruction("strb w19, [x15]");                                     // store the digit backwards
    emitter.instruction("mov w17, w18");                                        // continue with the reduced value
    emitter.instruction("cbnz w17, __rt_mb_encode_numericentity_utf8_digits");  // stop once every digit was produced
    emitter.label("__rt_mb_encode_numericentity_utf8_digit_copy");
    emitter.instruction("cmp x15, x12");                                        // are all digits copied?
    emitter.instruction("b.hs __rt_mb_encode_numericentity_utf8_semicolon");    // yes → terminate the reference
    emitter.instruction("ldrb w18, [x15], #1");                                 // load the next decimal digit
    emitter.instruction("strb w18, [x11], #1");                                 // place it in the result
    emitter.instruction("b __rt_mb_encode_numericentity_utf8_digit_copy");      // copy the next digit
    emitter.label("__rt_mb_encode_numericentity_utf8_semicolon");
    emitter.instruction("mov w18, #59");                                        // ASCII ';'
    emitter.instruction("strb w18, [x11], #1");                                 // terminate the numeric reference
    emitter.instruction("b __rt_mb_encode_numericentity_utf8_copy");            // inspect the next source codepoint
    emitter.label("__rt_mb_encode_numericentity_utf8_ascii");
    emitter.instruction("strb w12, [x11], #1");                                 // copy the ASCII byte unchanged
    emitter.instruction("add x1, x1, #1");                                      // consume one source byte
    emitter.instruction("sub x10, x10, #1");                                    // account for the consumed byte
    emitter.instruction("b __rt_mb_encode_numericentity_utf8_copy");            // inspect the next source byte
    emitter.label("__rt_mb_encode_numericentity_utf8_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return the owned result pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return the output byte length
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #112");                                    // release the encoder frame
    emitter.instruction("ret");                                                 // transfer the owned encoded string
    emitter.label("__rt_mb_encode_numericentity_utf8_fallback");
    emitter.instruction("mov x1, xzr");                                         // signal that the caller must use the bridge
    emitter.instruction("mov x2, xzr");                                         // no bytes for a rejected input
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #112");                                    // release the encoder frame
    emitter.instruction("ret");                                                 // return the fallback sentinel
}

/// Emits the x86_64 UTF-8 numeric-entity encoder.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_encode_numericentity_utf8 ---");
    emitter.label_global("__rt_mb_encode_numericentity_utf8");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 112");                                        // reserve the source, result, and digit scratch slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the borrowed source pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the borrowed source length
    emitter.instruction("call __rt_mb_encode_numericentity_len");               // rax = exact encoded length or -1 for invalid UTF-8
    emitter.instruction("cmp rax, -1");                                         // did the shared pre-scan decline the input?
    emitter.instruction("je __rt_mb_encode_numericentity_utf8_fallback_x86");   // invalid bytes keep the bridge's substitution behaviour
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the output length across allocation
    emitter.instruction("call __rt_str_new_owned");                             // allocate exactly the pre-scanned output length
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the destination base
    emitter.instruction("mov r11, rax");                                        // destination cursor
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the source cursor
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the remaining source bytes
    emitter.label("__rt_mb_encode_numericentity_utf8_copy_x86");
    emitter.instruction("test r9, r9");                                         // is the source exhausted?
    emitter.instruction("jz __rt_mb_encode_numericentity_utf8_done_x86");       // publish the encoded result
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                            // load the next source byte
    emitter.instruction("cmp ecx, 0x80");                                       // is this a plain ASCII byte?
    emitter.instruction("jb __rt_mb_encode_numericentity_utf8_ascii_x86");      // copy ASCII unchanged
    emitter.instruction("cmp ecx, 0xE0");                                       // is this a two-byte lead?
    emitter.instruction("jb __rt_mb_encode_numericentity_utf8_two_x86");        // decode the two-byte sequence
    emitter.instruction("cmp ecx, 0xF0");                                       // is this a three-byte lead?
    emitter.instruction("jb __rt_mb_encode_numericentity_utf8_three_x86");      // decode the three-byte sequence
    emitter.instruction("and ecx, 0x07");                                       // keep the four-byte payload bits
    emitter.instruction("movzx edx, BYTE PTR [r8 + 1]");                        // load the first continuation byte
    emitter.instruction("and edx, 0x3F");                                       // keep its payload
    emitter.instruction("shl ecx, 6");                                          // shift the accumulated payload
    emitter.instruction("or ecx, edx");                                         // append the first continuation payload
    emitter.instruction("movzx edx, BYTE PTR [r8 + 2]");                        // load the second continuation byte
    emitter.instruction("and edx, 0x3F");                                       // keep its payload
    emitter.instruction("shl ecx, 6");                                          // shift the accumulated payload
    emitter.instruction("or ecx, edx");                                         // append the second continuation payload
    emitter.instruction("movzx edx, BYTE PTR [r8 + 3]");                        // load the third continuation byte
    emitter.instruction("and edx, 0x3F");                                       // keep its payload
    emitter.instruction("shl ecx, 6");                                          // shift the accumulated payload
    emitter.instruction("or ecx, edx");                                         // append the third continuation payload
    emitter.instruction("add r8, 4");                                           // consume the four-byte sequence
    emitter.instruction("sub r9, 4");                                           // account for the consumed bytes
    emitter.instruction("jmp __rt_mb_encode_numericentity_utf8_entity_x86");    // emit the numeric reference
    emitter.label("__rt_mb_encode_numericentity_utf8_three_x86");
    emitter.instruction("and ecx, 0x0F");                                       // keep the three-byte payload bits
    emitter.instruction("movzx edx, BYTE PTR [r8 + 1]");                        // load the first continuation byte
    emitter.instruction("and edx, 0x3F");                                       // keep its payload
    emitter.instruction("shl ecx, 6");                                          // shift the accumulated payload
    emitter.instruction("or ecx, edx");                                         // append the first continuation payload
    emitter.instruction("movzx edx, BYTE PTR [r8 + 2]");                        // load the second continuation byte
    emitter.instruction("and edx, 0x3F");                                       // keep its payload
    emitter.instruction("shl ecx, 6");                                          // shift the accumulated payload
    emitter.instruction("or ecx, edx");                                         // append the second continuation payload
    emitter.instruction("add r8, 3");                                           // consume the three-byte sequence
    emitter.instruction("sub r9, 3");                                           // account for the consumed bytes
    emitter.instruction("jmp __rt_mb_encode_numericentity_utf8_entity_x86");    // emit the numeric reference
    emitter.label("__rt_mb_encode_numericentity_utf8_two_x86");
    emitter.instruction("and ecx, 0x1F");                                       // keep the two-byte payload bits
    emitter.instruction("movzx edx, BYTE PTR [r8 + 1]");                        // load the continuation byte
    emitter.instruction("and edx, 0x3F");                                       // keep its payload
    emitter.instruction("shl ecx, 6");                                          // shift the accumulated payload
    emitter.instruction("or ecx, edx");                                         // append the continuation payload
    emitter.instruction("add r8, 2");                                           // consume the two-byte sequence
    emitter.instruction("sub r9, 2");                                           // account for the consumed bytes
    emitter.label("__rt_mb_encode_numericentity_utf8_entity_x86");
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write the reference opener '&'
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("mov BYTE PTR [r11], 35");                              // write the decimal marker '#'
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("lea r10, [rbp - 88]");                                 // end of the 24-byte digit scratch
    emitter.instruction("mov rdi, r10");                                        // digits are built backwards from the end
    emitter.instruction("mov eax, ecx");                                        // decimal value to render
    emitter.instruction("mov esi, 10");                                         // decimal radix
    emitter.label("__rt_mb_encode_numericentity_utf8_digits_x86");
    emitter.instruction("xor edx, edx");                                        // clear the dividend high half
    emitter.instruction("div esi");                                             // eax = quotient, edx = removed digit
    emitter.instruction("add edx, 48");                                         // convert the digit to ASCII
    emitter.instruction("dec rdi");                                             // move the digit cursor down
    emitter.instruction("mov BYTE PTR [rdi], dl");                              // store the digit backwards
    emitter.instruction("test eax, eax");                                       // are more digits left?
    emitter.instruction("jnz __rt_mb_encode_numericentity_utf8_digits_x86");    // continue until the value is exhausted
    emitter.label("__rt_mb_encode_numericentity_utf8_digit_copy_x86");
    emitter.instruction("cmp rdi, r10");                                        // are all digits copied?
    emitter.instruction("jae __rt_mb_encode_numericentity_utf8_semicolon_x86"); // yes → terminate the reference
    emitter.instruction("movzx edx, BYTE PTR [rdi]");                           // load the next decimal digit
    emitter.instruction("mov BYTE PTR [r11], dl");                              // place it in the result
    emitter.instruction("inc rdi");                                             // advance the digit cursor
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("jmp __rt_mb_encode_numericentity_utf8_digit_copy_x86"); // copy the next digit
    emitter.label("__rt_mb_encode_numericentity_utf8_semicolon_x86");
    emitter.instruction("mov BYTE PTR [r11], 59");                              // terminate the numeric reference
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("jmp __rt_mb_encode_numericentity_utf8_copy_x86");      // inspect the next source codepoint
    emitter.label("__rt_mb_encode_numericentity_utf8_ascii_x86");
    emitter.instruction("mov BYTE PTR [r11], cl");                              // copy the ASCII byte unchanged
    emitter.instruction("inc r11");                                             // advance the destination cursor
    emitter.instruction("inc r8");                                              // consume one source byte
    emitter.instruction("dec r9");                                              // account for the consumed byte
    emitter.instruction("jmp __rt_mb_encode_numericentity_utf8_copy_x86");      // inspect the next source byte
    emitter.label("__rt_mb_encode_numericentity_utf8_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the owned result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // return the output byte length
    emitter.instruction("leave");                                               // restore the caller frame pointer
    emitter.instruction("ret");                                                 // transfer the owned encoded string
    emitter.label("__rt_mb_encode_numericentity_utf8_fallback_x86");
    emitter.instruction("xor eax, eax");                                        // signal that the caller must use the bridge
    emitter.instruction("xor edx, edx");                                        // no bytes for a rejected input
    emitter.instruction("leave");                                               // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the fallback sentinel
}
