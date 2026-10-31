//! Purpose:
//! Emits the allocation-free numeric-entity length helpers used by the explicit full-range
//! `mb_encode_numericentity()` and `mb_decode_numericentity()` fast paths.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::mbstring::emit_mbstring()`.
//!
//! Key details:
//! - The encode helper accepts valid UTF-8 only and returns the decimal entity length for
//!   every non-ASCII codepoint; invalid input returns `-1` so the caller falls back to the
//!   shared mbstring bridge.
//! - The decode helper accepts well-formed `&#NNN;` decimal entities inside the full Unicode
//!   range; any other entity shape returns `-1` and falls back.
//! - AArch64 input: `x1`/`x2` = source pointer/length; result in `x0`. x86_64 input:
//!   `rax`/`rdx`; result in `rax`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits both numeric-entity length helpers for the active target.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits the AArch64 encode and decode length helpers.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_encode_numericentity_len ---");
    emitter.label_global("__rt_mb_encode_numericentity_len");
    emitter.instruction("mov x0, xzr");                                          // encoded length starts at zero
    emitter.instruction("mov x9, x1");                                          // source cursor
    emitter.instruction("mov x10, x2");                                        // remaining source bytes
    emitter.label("__rt_mb_encode_numericentity_len_loop");
    emitter.instruction("cbz x10, __rt_mb_encode_numericentity_len_done");     // stop after the final source byte
    emitter.instruction("ldrb w11, [x9], #1");                                  // load and consume one source byte
    emitter.instruction("sub x10, x10, #1");                                    // account for the consumed byte
    emitter.instruction("cmp w11, #0x80");                                      // is this a plain ASCII byte?
    emitter.instruction("b.lo __rt_mb_encode_numericentity_len_ascii");        // copy ASCII as one byte
    emitter.instruction("cmp w11, #0xC2");                                      // is this a valid UTF-8 lead?
    emitter.instruction("b.lo __rt_mb_encode_numericentity_len_invalid");      // continuations and overlong leads fall back
    emitter.instruction("cmp w11, #0xE0");                                      // is this a two-byte lead?
    emitter.instruction("b.lo __rt_mb_encode_numericentity_len_two");          // decode the two-byte sequence
    emitter.instruction("cmp w11, #0xF0");                                      // is this a three-byte lead?
    emitter.instruction("b.lo __rt_mb_encode_numericentity_len_three");        // decode the three-byte sequence
    emitter.instruction("cmp w11, #0xF5");                                      // is this a valid four-byte lead?
    emitter.instruction("b.hs __rt_mb_encode_numericentity_len_invalid");      // beyond U+10FFFF falls back
    emitter.instruction("mov x15, #3");                                         // expected continuation bytes
    emitter.instruction("and x12, x11, #0x07");                                 // keep the four-byte payload bits
    emitter.instruction("b __rt_mb_encode_numericentity_len_consume");         // decode the continuation bytes
    emitter.label("__rt_mb_encode_numericentity_len_two");
    emitter.instruction("mov x15, #1");                                         // expected continuation bytes
    emitter.instruction("and x12, x11, #0x1F");                                 // keep the two-byte payload bits
    emitter.instruction("b __rt_mb_encode_numericentity_len_consume");         // decode the continuation bytes
    emitter.label("__rt_mb_encode_numericentity_len_three");
    emitter.instruction("mov x15, #2");                                         // expected continuation bytes
    emitter.instruction("and x12, x11, #0x0F");                                 // keep the three-byte payload bits
    emitter.label("__rt_mb_encode_numericentity_len_consume");
    emitter.instruction("cbz x15, __rt_mb_encode_numericentity_len_validate"); // all continuation bytes consumed
    emitter.instruction("cbz x10, __rt_mb_encode_numericentity_len_invalid");  // a truncated sequence falls back
    emitter.instruction("ldrb w13, [x9]");                                      // peek at the next continuation byte
    emitter.instruction("sub w14, w13, #0x80");                                 // map it to 0..63
    emitter.instruction("cmp w14, #0x3F");                                      // is this a continuation byte?
    emitter.instruction("b.hi __rt_mb_encode_numericentity_len_invalid");      // a non-continuation byte falls back
    emitter.instruction("add x9, x9, #1");                                      // consume the continuation byte
    emitter.instruction("sub x10, x10, #1");                                    // account for the consumed byte
    emitter.instruction("lsl x12, x12, #6");                                    // shift the accumulated payload
    emitter.instruction("and x13, x13, #0x3F");                                 // keep the continuation payload
    emitter.instruction("orr x12, x12, x13");                                   // append the continuation payload
    emitter.instruction("sub x15, x15, #1");                                    // one fewer continuation expected
    emitter.instruction("b __rt_mb_encode_numericentity_len_consume");         // consume the next continuation byte
    emitter.label("__rt_mb_encode_numericentity_len_validate");
    emitter.instruction("cmp w11, #0xE0");                                      // was this a two-byte sequence?
    emitter.instruction("b.lo __rt_mb_encode_numericentity_len_entity");       // two-byte codepoints are always valid
    emitter.instruction("cmp w11, #0xF0");                                      // was this a three-byte sequence?
    emitter.instruction("b.lo __rt_mb_encode_numericentity_len_check_three");  // apply the three-byte range checks
    emitter.instruction("movz w14, #1, lsl #16");                              // first four-byte codepoint
    emitter.instruction("cmp x12, x14");                                       // reject overlong four-byte sequences
    emitter.instruction("b.lo __rt_mb_encode_numericentity_len_invalid");      // overlong input falls back
    emitter.instruction("movz w14, #0x11, lsl #16");                           // one past the last Unicode codepoint
    emitter.instruction("cmp x12, x14");                                       // reject codepoints beyond Unicode
    emitter.instruction("b.hs __rt_mb_encode_numericentity_len_invalid");      // out-of-range input falls back
    emitter.instruction("b __rt_mb_encode_numericentity_len_entity");          // encode the valid codepoint
    emitter.label("__rt_mb_encode_numericentity_len_check_three");
    emitter.instruction("mov x14, #0x800");                                     // first three-byte codepoint
    emitter.instruction("cmp x12, x14");                                       // reject overlong three-byte sequences
    emitter.instruction("b.lo __rt_mb_encode_numericentity_len_invalid");      // overlong input falls back
    emitter.instruction("mov x14, #0xD800");                                    // first UTF-16 surrogate
    emitter.instruction("cmp x12, x14");                                       // is this below the surrogate range?
    emitter.instruction("b.lo __rt_mb_encode_numericentity_len_entity");       // a valid three-byte codepoint
    emitter.instruction("mov x14, #0xE000");                                    // first post-surrogate codepoint
    emitter.instruction("cmp x12, x14");                                       // is this inside the surrogate range?
    emitter.instruction("b.lo __rt_mb_encode_numericentity_len_invalid");      // surrogates fall back
    emitter.label("__rt_mb_encode_numericentity_len_entity");
    emitter.instruction("mov x14, #1");                                         // decimal digit count starts at one
    emitter.label("__rt_mb_encode_numericentity_len_digits");
    emitter.instruction("cmp x12, #10");                                        // is the codepoint a single digit?
    emitter.instruction("b.lo __rt_mb_encode_numericentity_len_digits_done");  // stop once the digit count is complete
    emitter.instruction("mov x15, #10");                                        // decimal radix
    emitter.instruction("udiv x12, x12, x15");                                  // drop the least significant digit
    emitter.instruction("add x14, x14, #1");                                    // count the removed digit
    emitter.instruction("b __rt_mb_encode_numericentity_len_digits");          // continue until one digit remains
    emitter.label("__rt_mb_encode_numericentity_len_digits_done");
    emitter.instruction("add x0, x0, #3");                                       // "&#" plus ";"
    emitter.instruction("add x0, x0, x14");                                      // add the decimal digits
    emitter.instruction("b __rt_mb_encode_numericentity_len_loop");            // inspect the next source codepoint
    emitter.label("__rt_mb_encode_numericentity_len_ascii");
    emitter.instruction("add x0, x0, #1");                                       // ASCII stays one byte
    emitter.instruction("b __rt_mb_encode_numericentity_len_loop");            // inspect the next source byte
    emitter.label("__rt_mb_encode_numericentity_len_invalid");
    emitter.instruction("mov x0, #-1");                                          // signal that the caller must use the bridge
    emitter.instruction("ret");                                                // return the fallback sentinel
    emitter.label("__rt_mb_encode_numericentity_len_done");
    emitter.instruction("ret");                                                // return the encoded byte length

    emitter.blank();
    emitter.comment("--- runtime: mb_decode_numericentity_len ---");
    emitter.label_global("__rt_mb_decode_numericentity_len");
    emitter.instruction("mov x0, xzr");                                          // decoded length starts at zero
    emitter.instruction("mov x9, x1");                                          // source cursor
    emitter.instruction("mov x10, x2");                                        // remaining source bytes
    emitter.label("__rt_mb_decode_numericentity_len_loop");
    emitter.instruction("cbz x10, __rt_mb_decode_numericentity_len_done");     // stop after the final source byte
    emitter.instruction("ldrb w11, [x9]");                                      // peek at the next source byte
    emitter.instruction("cmp w11, #'&'");                                       // does a numeric entity start here?
    emitter.instruction("b.ne __rt_mb_decode_numericentity_len_copy");        // ordinary bytes copy one for one
    emitter.instruction("cmp x10, #4");                                         // is a minimal entity present?
    emitter.instruction("b.lo __rt_mb_decode_numericentity_len_invalid");      // a short entity falls back
    emitter.instruction("ldrb w11, [x9, #1]");                                  // load the entity marker
    emitter.instruction("cmp w11, #'#'");                                       // is this a decimal numeric entity?
    emitter.instruction("b.ne __rt_mb_decode_numericentity_len_invalid");      // non-decimal entities fall back
    emitter.instruction("add x12, x9, #2");                                      // digit cursor
    emitter.instruction("sub x13, x10, #2");                                    // remaining bytes after "&#"
    emitter.instruction("mov x14, xzr");                                        // parsed codepoint
    emitter.instruction("mov x15, xzr");                                        // parsed digit count
    emitter.label("__rt_mb_decode_numericentity_len_digits");
    emitter.instruction("cbz x13, __rt_mb_decode_numericentity_len_invalid");  // a missing semicolon falls back
    emitter.instruction("ldrb w11, [x12]");                                     // load the next entity byte
    emitter.instruction("cmp w11, #'0'");                                       // is this below a decimal digit?
    emitter.instruction("b.lo __rt_mb_decode_numericentity_len_digits_end");   // the digit run ended
    emitter.instruction("cmp w11, #'9'");                                       // is this above a decimal digit?
    emitter.instruction("b.hi __rt_mb_decode_numericentity_len_digits_end");   // the digit run ended
    emitter.instruction("sub w11, w11, #'0'");                                  // convert the digit to 0..9
    emitter.instruction("mov x16, #10");                                        // decimal radix
    emitter.instruction("mul x14, x14, x16");                                    // shift the parsed codepoint
    emitter.instruction("add x14, x14, x11");                                    // append the decimal digit
    emitter.instruction("movz w16, #0x11, lsl #16");                           // one past the last Unicode codepoint
    emitter.instruction("cmp x14, x16");                                        // reject codepoints beyond Unicode
    emitter.instruction("b.hs __rt_mb_decode_numericentity_len_invalid");      // out-of-range entities fall back
    emitter.instruction("add x12, x12, #1");                                     // advance the digit cursor
    emitter.instruction("sub x13, x13, #1");                                     // account for the consumed digit
    emitter.instruction("add x15, x15, #1");                                     // count the digit
    emitter.instruction("b __rt_mb_decode_numericentity_len_digits");          // parse the next digit
    emitter.label("__rt_mb_decode_numericentity_len_digits_end");
    emitter.instruction("cbz x15, __rt_mb_decode_numericentity_len_invalid");  // an empty digit run falls back
    emitter.instruction("ldrb w11, [x12]");                                     // load the entity terminator
    emitter.instruction("cmp w11, #';'");                                       // is this a decimal entity terminator?
    emitter.instruction("b.ne __rt_mb_decode_numericentity_len_invalid");      // malformed entities fall back
    emitter.instruction("add x16, x15, #3");                                    // consumed bytes = "&#" + digits + ";"
    emitter.instruction("cmp x14, #0x80");                                      // is this inside the encoded map range?
    emitter.instruction("b.lo __rt_mb_decode_numericentity_len_invalid");      // ASCII entities fall back
    emitter.instruction("cmp x14, #0x800");                                     // does the codepoint need two UTF-8 bytes?
    emitter.instruction("b.lo __rt_mb_decode_numericentity_len_two");          // a two-byte codepoint
    emitter.instruction("movz w16, #0xD800");                                   // first UTF-16 surrogate
    emitter.instruction("cmp x14, x16");                                       // is this below the surrogate range?
    emitter.instruction("b.lo __rt_mb_decode_numericentity_len_three");        // a three-byte codepoint
    emitter.instruction("movz w16, #0xE000");                                  // first post-surrogate codepoint
    emitter.instruction("cmp x14, x16");                                       // is this inside the surrogate range?
    emitter.instruction("b.lo __rt_mb_decode_numericentity_len_invalid");      // surrogates fall back
    emitter.instruction("movz w16, #1, lsl #16");                              // first four-byte codepoint
    emitter.instruction("cmp x14, x16");                                       // does the codepoint need three UTF-8 bytes?
    emitter.instruction("b.lo __rt_mb_decode_numericentity_len_three");        // a three-byte codepoint
    emitter.instruction("add x0, x0, #4");                                       // a four-byte codepoint
    emitter.instruction("b __rt_mb_decode_numericentity_len_advance");         // consume the entity
    emitter.label("__rt_mb_decode_numericentity_len_two");
    emitter.instruction("add x0, x0, #2");                                       // a two-byte codepoint
    emitter.instruction("b __rt_mb_decode_numericentity_len_advance");         // consume the entity
    emitter.label("__rt_mb_decode_numericentity_len_three");
    emitter.instruction("add x0, x0, #3");                                       // a three-byte codepoint
    emitter.label("__rt_mb_decode_numericentity_len_advance");
    emitter.instruction("add x9, x9, x16");                                      // skip the consumed entity
    emitter.instruction("sub x10, x10, x16");                                    // account for the consumed entity
    emitter.instruction("b __rt_mb_decode_numericentity_len_loop");             // decode the next source byte
    emitter.label("__rt_mb_decode_numericentity_len_copy");
    emitter.instruction("add x0, x0, #1");                                       // ordinary bytes decode one for one
    emitter.instruction("add x9, x9, #1");                                       // consume one source byte
    emitter.instruction("sub x10, x10, #1");                                     // account for the consumed byte
    emitter.instruction("b __rt_mb_decode_numericentity_len_loop");             // decode the next source byte
    emitter.label("__rt_mb_decode_numericentity_len_invalid");
    emitter.instruction("mov x0, #-1");                                          // signal that the caller must use the bridge
    emitter.instruction("ret");                                                // return the fallback sentinel
    emitter.label("__rt_mb_decode_numericentity_len_done");
    emitter.instruction("ret");                                                // return the decoded byte length
}

/// Emits the x86_64 encode and decode length helpers.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_encode_numericentity_len ---");
    emitter.label_global("__rt_mb_encode_numericentity_len");
    emitter.instruction("xor esi, esi");                                        // encoded length starts at zero
    emitter.instruction("mov r8, rax");                                         // source cursor
    emitter.instruction("mov r9, rdx");                                        // remaining source bytes
    emitter.label("__rt_mb_encode_numericentity_len_loop_x86");
    emitter.instruction("test r9, r9");                                        // is the source exhausted?
    emitter.instruction("jz __rt_mb_encode_numericentity_len_done_x86");       // return the encoded length
    emitter.instruction("movzx r10d, BYTE PTR [r8]");                          // load one source byte
    emitter.instruction("inc r8");                                            // consume the source byte
    emitter.instruction("dec r9");                                            // account for the consumed byte
    emitter.instruction("cmp r10d, 0x80");                                     // is this a plain ASCII byte?
    emitter.instruction("jb __rt_mb_encode_numericentity_len_ascii_x86");     // copy ASCII as one byte
    emitter.instruction("cmp r10d, 0xC2");                                     // is this a valid UTF-8 lead?
    emitter.instruction("jb __rt_mb_encode_numericentity_len_invalid_x86");   // continuations and overlong leads fall back
    emitter.instruction("cmp r10d, 0xE0");                                     // is this a two-byte lead?
    emitter.instruction("jb __rt_mb_encode_numericentity_len_two_x86");       // decode the two-byte sequence
    emitter.instruction("cmp r10d, 0xF0");                                     // is this a three-byte lead?
    emitter.instruction("jb __rt_mb_encode_numericentity_len_three_x86");     // decode the three-byte sequence
    emitter.instruction("cmp r10d, 0xF5");                                     // is this a valid four-byte lead?
    emitter.instruction("jae __rt_mb_encode_numericentity_len_invalid_x86");  // beyond U+10FFFF falls back
    emitter.instruction("mov edi, 3");                                         // expected continuation bytes
    emitter.instruction("mov r11d, r10d");                                      // copy the lead byte
    emitter.instruction("and r11d, 0x07");                                      // keep the four-byte payload bits
    emitter.instruction("jmp __rt_mb_encode_numericentity_len_consume_x86");  // decode the continuation bytes
    emitter.label("__rt_mb_encode_numericentity_len_two_x86");
    emitter.instruction("mov edi, 1");                                         // expected continuation bytes
    emitter.instruction("mov r11d, r10d");                                      // copy the lead byte
    emitter.instruction("and r11d, 0x1F");                                      // keep the two-byte payload bits
    emitter.instruction("jmp __rt_mb_encode_numericentity_len_consume_x86");  // decode the continuation bytes
    emitter.label("__rt_mb_encode_numericentity_len_three_x86");
    emitter.instruction("mov edi, 2");                                         // expected continuation bytes
    emitter.instruction("mov r11d, r10d");                                      // copy the lead byte
    emitter.instruction("and r11d, 0x0F");                                      // keep the three-byte payload bits
    emitter.label("__rt_mb_encode_numericentity_len_consume_x86");
    emitter.instruction("test edi, edi");                                      // are all continuation bytes consumed?
    emitter.instruction("jz __rt_mb_encode_numericentity_len_validate_x86");   // validate the codepoint
    emitter.instruction("test r9, r9");                                        // is another continuation byte present?
    emitter.instruction("jz __rt_mb_encode_numericentity_len_invalid_x86");   // a truncated sequence falls back
    emitter.instruction("movzx ecx, BYTE PTR [r8]");                          // peek at the next continuation byte
    emitter.instruction("mov eax, ecx");                                       // copy the continuation byte
    emitter.instruction("sub eax, 0x80");                                      // map it to 0..63
    emitter.instruction("cmp eax, 0x3F");                                      // is this a continuation byte?
    emitter.instruction("ja __rt_mb_encode_numericentity_len_invalid_x86");  // a non-continuation byte falls back
    emitter.instruction("inc r8");                                            // consume the continuation byte
    emitter.instruction("dec r9");                                            // account for the consumed byte
    emitter.instruction("shl r11d, 6");                                        // shift the accumulated payload
    emitter.instruction("and ecx, 0x3F");                                      // keep the continuation payload
    emitter.instruction("or r11d, ecx");                                       // append the continuation payload
    emitter.instruction("dec edi");                                           // one fewer continuation expected
    emitter.instruction("jmp __rt_mb_encode_numericentity_len_consume_x86");  // consume the next continuation byte
    emitter.label("__rt_mb_encode_numericentity_len_validate_x86");
    emitter.instruction("cmp r10d, 0xE0");                                     // was this a two-byte sequence?
    emitter.instruction("jb __rt_mb_encode_numericentity_len_entity_x86");    // two-byte codepoints are always valid
    emitter.instruction("cmp r10d, 0xF0");                                     // was this a three-byte sequence?
    emitter.instruction("jb __rt_mb_encode_numericentity_len_check_three_x86"); // apply the three-byte range checks
    emitter.instruction("cmp r11d, 0x10000");                                  // reject overlong four-byte sequences
    emitter.instruction("jb __rt_mb_encode_numericentity_len_invalid_x86");  // overlong input falls back
    emitter.instruction("cmp r11d, 0x110000");                                 // reject codepoints beyond Unicode
    emitter.instruction("jae __rt_mb_encode_numericentity_len_invalid_x86"); // out-of-range input falls back
    emitter.instruction("jmp __rt_mb_encode_numericentity_len_entity_x86");  // encode the valid codepoint
    emitter.label("__rt_mb_encode_numericentity_len_check_three_x86");
    emitter.instruction("cmp r11d, 0x800");                                     // reject overlong three-byte sequences
    emitter.instruction("jb __rt_mb_encode_numericentity_len_invalid_x86");  // overlong input falls back
    emitter.instruction("cmp r11d, 0xD800");                                   // is this below the surrogate range?
    emitter.instruction("jb __rt_mb_encode_numericentity_len_entity_x86");   // a valid three-byte codepoint
    emitter.instruction("cmp r11d, 0xE000");                                   // is this inside the surrogate range?
    emitter.instruction("jb __rt_mb_encode_numericentity_len_invalid_x86");  // surrogates fall back
    emitter.label("__rt_mb_encode_numericentity_len_entity_x86");
    emitter.instruction("mov edi, 1");                                         // decimal digit count starts at one
    emitter.label("__rt_mb_encode_numericentity_len_digits_x86");
    emitter.instruction("cmp r11d, 10");                                       // is the codepoint a single digit?
    emitter.instruction("jb __rt_mb_encode_numericentity_len_digits_done_x86"); // stop once the digit count is complete
    emitter.instruction("mov eax, r11d");                                      // dividend
    emitter.instruction("xor edx, edx");                                      // clear the dividend high half
    emitter.instruction("mov ecx, 10");                                       // decimal radix
    emitter.instruction("div ecx");                                           // drop the least significant digit
    emitter.instruction("mov r11d, eax");                                      // keep the reduced codepoint
    emitter.instruction("inc edi");                                            // count the removed digit
    emitter.instruction("jmp __rt_mb_encode_numericentity_len_digits_x86");  // continue until one digit remains
    emitter.label("__rt_mb_encode_numericentity_len_digits_done_x86");
    emitter.instruction("add esi, 3");                                         // "&#" plus ";"
    emitter.instruction("add esi, edi");                                       // add the decimal digits
    emitter.instruction("jmp __rt_mb_encode_numericentity_len_loop_x86");    // inspect the next source codepoint
    emitter.label("__rt_mb_encode_numericentity_len_ascii_x86");
    emitter.instruction("inc esi");                                            // ASCII stays one byte
    emitter.instruction("jmp __rt_mb_encode_numericentity_len_loop_x86");    // inspect the next source byte
    emitter.label("__rt_mb_encode_numericentity_len_invalid_x86");
    emitter.instruction("mov rax, -1");                                        // signal that the caller must use the bridge
    emitter.instruction("ret");                                              // return the fallback sentinel
    emitter.label("__rt_mb_encode_numericentity_len_done_x86");
    emitter.instruction("mov rax, rsi");                                        // return the encoded byte length
    emitter.instruction("ret");                                              // return to the caller

    emitter.blank();
    emitter.comment("--- runtime: mb_decode_numericentity_len ---");
    emitter.label_global("__rt_mb_decode_numericentity_len");
    emitter.instruction("xor esi, esi");                                        // decoded length starts at zero
    emitter.instruction("mov r8, rax");                                         // source cursor
    emitter.instruction("mov r9, rdx");                                        // remaining source bytes
    emitter.label("__rt_mb_decode_numericentity_len_loop_x86");
    emitter.instruction("test r9, r9");                                        // is the source exhausted?
    emitter.instruction("jz __rt_mb_decode_numericentity_len_done_x86");       // return the decoded length
    emitter.instruction("cmp BYTE PTR [r8], '&'");                             // does a numeric entity start here?
    emitter.instruction("jne __rt_mb_decode_numericentity_len_copy_x86");     // ordinary bytes decode one for one
    emitter.instruction("cmp r9, 4");                                          // is a minimal entity present?
    emitter.instruction("jb __rt_mb_decode_numericentity_len_invalid_x86");   // a short entity falls back
    emitter.instruction("cmp BYTE PTR [r8 + 1], '#'");                         // is this a decimal numeric entity?
    emitter.instruction("jne __rt_mb_decode_numericentity_len_invalid_x86"); // non-decimal entities fall back
    emitter.instruction("lea r11, [r8 + 2]");                                  // digit cursor
    emitter.instruction("mov rcx, r9");                                        // remaining source bytes
    emitter.instruction("sub rcx, 2");                                         // bytes after "&#"
    emitter.instruction("xor edx, edx");                                       // parsed codepoint
    emitter.instruction("xor edi, edi");                                       // parsed digit count
    emitter.label("__rt_mb_decode_numericentity_len_digits_x86");
    emitter.instruction("test rcx, rcx");                                      // is the entity truncated?
    emitter.instruction("jz __rt_mb_decode_numericentity_len_invalid_x86");   // a missing semicolon falls back
    emitter.instruction("movzx r10d, BYTE PTR [r11]");                         // load the next entity byte
    emitter.instruction("cmp r10d, '0'");                                      // is this below a decimal digit?
    emitter.instruction("jb __rt_mb_decode_numericentity_len_digits_end_x86"); // the digit run ended
    emitter.instruction("cmp r10d, '9'");                                      // is this above a decimal digit?
    emitter.instruction("ja __rt_mb_decode_numericentity_len_digits_end_x86"); // the digit run ended
    emitter.instruction("sub r10d, '0'");                                      // convert the digit to 0..9
    emitter.instruction("imul rdx, rdx, 10");                                  // shift the parsed codepoint
    emitter.instruction("add rdx, r10");                                       // append the decimal digit
    emitter.instruction("cmp rdx, 0x110000");                                  // reject codepoints beyond Unicode
    emitter.instruction("jae __rt_mb_decode_numericentity_len_invalid_x86"); // out-of-range entities fall back
    emitter.instruction("inc r11");                                           // advance the digit cursor
    emitter.instruction("dec rcx");                                           // account for the consumed digit
    emitter.instruction("inc rdi");                                           // count the digit
    emitter.instruction("jmp __rt_mb_decode_numericentity_len_digits_x86");  // parse the next digit
    emitter.label("__rt_mb_decode_numericentity_len_digits_end_x86");
    emitter.instruction("test rdi, rdi");                                      // is the digit run empty?
    emitter.instruction("jz __rt_mb_decode_numericentity_len_invalid_x86");   // an empty digit run falls back
    emitter.instruction("cmp BYTE PTR [r11], ';'");                           // is this a decimal entity terminator?
    emitter.instruction("jne __rt_mb_decode_numericentity_len_invalid_x86"); // malformed entities fall back
    emitter.instruction("lea rax, [rdi + 3]");                                 // consumed bytes = "&#" + digits + ";"
    emitter.instruction("cmp rdx, 0x80");                                      // is this inside the encoded map range?
    emitter.instruction("jb __rt_mb_decode_numericentity_len_invalid_x86");  // ASCII entities fall back
    emitter.instruction("cmp rdx, 0x800");                                     // does the codepoint need two UTF-8 bytes?
    emitter.instruction("jb __rt_mb_decode_numericentity_len_two_x86");       // a two-byte codepoint
    emitter.instruction("cmp rdx, 0xD800");                                    // is this below the surrogate range?
    emitter.instruction("jb __rt_mb_decode_numericentity_len_three_x86");     // a three-byte codepoint
    emitter.instruction("cmp rdx, 0xE000");                                    // is this inside the surrogate range?
    emitter.instruction("jb __rt_mb_decode_numericentity_len_invalid_x86");  // surrogates fall back
    emitter.instruction("cmp rdx, 0x10000");                                   // does the codepoint need three UTF-8 bytes?
    emitter.instruction("jb __rt_mb_decode_numericentity_len_three_x86");     // a three-byte codepoint
    emitter.instruction("add rsi, 4");                                         // a four-byte codepoint
    emitter.instruction("jmp __rt_mb_decode_numericentity_len_advance_x86"); // consume the entity
    emitter.label("__rt_mb_decode_numericentity_len_two_x86");
    emitter.instruction("add rsi, 2");                                         // a two-byte codepoint
    emitter.instruction("jmp __rt_mb_decode_numericentity_len_advance_x86"); // consume the entity
    emitter.label("__rt_mb_decode_numericentity_len_three_x86");
    emitter.instruction("add rsi, 3");                                         // a three-byte codepoint
    emitter.label("__rt_mb_decode_numericentity_len_advance_x86");
    emitter.instruction("add r8, rax");                                        // skip the consumed entity
    emitter.instruction("sub r9, rax");                                        // account for the consumed entity
    emitter.instruction("jmp __rt_mb_decode_numericentity_len_loop_x86");    // decode the next source byte
    emitter.label("__rt_mb_decode_numericentity_len_copy_x86");
    emitter.instruction("inc rsi");                                           // ordinary bytes decode one for one
    emitter.instruction("inc r8");                                            // consume one source byte
    emitter.instruction("dec r9");                                            // account for the consumed byte
    emitter.instruction("jmp __rt_mb_decode_numericentity_len_loop_x86");    // decode the next source byte
    emitter.label("__rt_mb_decode_numericentity_len_invalid_x86");
    emitter.instruction("mov rax, -1");                                        // signal that the caller must use the bridge
    emitter.instruction("ret");                                              // return the fallback sentinel
    emitter.label("__rt_mb_decode_numericentity_len_done_x86");
    emitter.instruction("mov rax, rsi");                                        // return the decoded byte length
    emitter.instruction("ret");                                              // return to the caller
}
