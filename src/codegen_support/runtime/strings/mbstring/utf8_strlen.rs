//! Purpose:
//! Emits `__rt_mb_strlen_utf8`, the allocation-free UTF-8 codepoint counter used by the
//! explicit-encoding `mb_strlen($s, "UTF-8")` fast path.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::mbstring::emit_mbstring()`.
//!
//! Key details:
//! - The helper returns the number of codepoints, or `-1` when the subject is not valid
//!   UTF-8. The caller falls back to the shared mbstring coordinator, which reproduces
//!   PHP's replacement-character counting for malformed input.
//! - Sixteen-byte blocks of ASCII bytes are counted without a per-byte branch; the scalar
//!   path validates lead bytes, continuation bytes, overlong forms, surrogates and the
//!   Unicode range before counting a codepoint.
//! - ABI: AArch64 `x1`/`x2` = subject pointer/length, result in `x0`; x86_64 `rax`/`rdx`
//!   = subject pointer/length, result in `rax`. The helper is a leaf and clobbers only
//!   caller-saved registers.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the UTF-8 codepoint counter for the active target.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// Emits the AArch64 UTF-8 counter.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_strlen_utf8 ---");
    emitter.label_global("__rt_mb_strlen_utf8");
    emitter.instruction("mov x0, xzr");                                         // codepoint count starts at zero
    emitter.instruction("mov x9, x1");                                         // subject cursor
    emitter.instruction("mov x10, x2");                                        // remaining subject bytes
    emitter.label("__rt_mb_strlen_utf8_block");
    emitter.instruction("cmp x10, #16");                                       // is a full ASCII block available?
    emitter.instruction("b.lo __rt_mb_strlen_utf8_scalar");                    // the tail is handled byte by byte
    emitter.instruction("ldr q0, [x9]");                                       // load sixteen subject bytes
    emitter.instruction("cmlt v1.16b, v0.16b, #0");                             // mark bytes with the high bit set
    emitter.instruction("umaxv b1, v1.16b");                                    // did any byte leave the ASCII range?
    emitter.instruction("umov w11, v1.b[0]");                                  // move the block verdict into an integer register
    emitter.instruction("cbnz w11, __rt_mb_strlen_utf8_scalar");               // non-ASCII bytes use the validator
    emitter.instruction("add x0, x0, #16");                                    // count sixteen ASCII codepoints
    emitter.instruction("add x9, x9, #16");                                    // consume the ASCII block
    emitter.instruction("sub x10, x10, #16");                                  // account for the consumed block
    emitter.instruction("b __rt_mb_strlen_utf8_block");                        // scan the next block
    emitter.label("__rt_mb_strlen_utf8_scalar");
    emitter.instruction("cbz x10, __rt_mb_strlen_utf8_done");                  // an exhausted subject is valid UTF-8
    emitter.instruction("ldrb w11, [x9]");                                     // load the next lead byte
    emitter.instruction("cmp w11, #0x80");                                     // is this an ASCII byte?
    emitter.instruction("b.hs __rt_mb_strlen_utf8_non_ascii");                 // multibyte sequences take the validator
    emitter.instruction("add x0, x0, #1");                                     // count one ASCII codepoint
    emitter.instruction("add x9, x9, #1");                                     // consume the ASCII byte
    emitter.instruction("sub x10, x10, #1");                                   // account for the consumed byte
    emitter.instruction("b __rt_mb_strlen_utf8_block");                        // scan the next block
    emitter.label("__rt_mb_strlen_utf8_non_ascii");
    emitter.instruction("cmp w11, #0xC2");                                     // is this a valid two-byte lead?
    emitter.instruction("b.lo __rt_mb_strlen_utf8_invalid");                   // continuation or overlong lead bytes are invalid
    emitter.instruction("cmp w11, #0xE0");                                     // is this a two-byte lead?
    emitter.instruction("b.lo __rt_mb_strlen_utf8_two");                       // validate the two-byte sequence
    emitter.instruction("cmp w11, #0xF0");                                     // is this a three-byte lead?
    emitter.instruction("b.lo __rt_mb_strlen_utf8_three");                     // validate the three-byte sequence
    emitter.instruction("cmp w11, #0xF5");                                     // is this a valid four-byte lead?
    emitter.instruction("b.hs __rt_mb_strlen_utf8_invalid");                   // beyond U+10FFFF is invalid
    emitter.instruction("cmp x10, #4");                                       // is a complete four-byte sequence present?
    emitter.instruction("b.lo __rt_mb_strlen_utf8_invalid");                   // a truncated sequence is invalid
    emitter.instruction("ldrb w12, [x9, #1]");                                 // load the second byte
    emitter.instruction("sub w15, w12, #0x80");                                // map a continuation byte to 0..63
    emitter.instruction("cmp w15, #0x3F");                                     // is this a continuation byte?
    emitter.instruction("b.hi __rt_mb_strlen_utf8_invalid");                   // a non-continuation byte is invalid
    emitter.instruction("cmp w11, #0xF0");                                     // does the first byte constrain the second?
    emitter.instruction("b.ne __rt_mb_strlen_utf8_four_not_f0");               // only U+10000..U+3FFFF uses F0
    emitter.instruction("cmp w15, #0x10");                                     // F0 requires a second byte of at least 0x90
    emitter.instruction("b.lo __rt_mb_strlen_utf8_invalid");                   // an overlong sequence is invalid
    emitter.label("__rt_mb_strlen_utf8_four_not_f0");
    emitter.instruction("cmp w11, #0xF4");                                     // does the first byte constrain the second?
    emitter.instruction("b.ne __rt_mb_strlen_utf8_four_tail");                 // only F4 needs the upper range check
    emitter.instruction("cmp w15, #0x0F");                                     // F4 requires a second byte of at most 0x8F
    emitter.instruction("b.hi __rt_mb_strlen_utf8_invalid");                   // beyond U+10FFFF is invalid
    emitter.label("__rt_mb_strlen_utf8_four_tail");
    emitter.instruction("ldrb w13, [x9, #2]");                                 // load the third byte
    emitter.instruction("sub w15, w13, #0x80");                                // map a continuation byte to 0..63
    emitter.instruction("cmp w15, #0x3F");                                     // is this a continuation byte?
    emitter.instruction("b.hi __rt_mb_strlen_utf8_invalid");                   // a non-continuation byte is invalid
    emitter.instruction("ldrb w14, [x9, #3]");                                 // load the fourth byte
    emitter.instruction("sub w15, w14, #0x80");                                // map a continuation byte to 0..63
    emitter.instruction("cmp w15, #0x3F");                                     // is this a continuation byte?
    emitter.instruction("b.hi __rt_mb_strlen_utf8_invalid");                   // a non-continuation byte is invalid
    emitter.instruction("add x0, x0, #1");                                     // count one four-byte codepoint
    emitter.instruction("add x9, x9, #4");                                     // consume the sequence
    emitter.instruction("sub x10, x10, #4");                                   // account for the consumed bytes
    emitter.instruction("b __rt_mb_strlen_utf8_block");                        // scan the next block
    emitter.label("__rt_mb_strlen_utf8_two");
    emitter.instruction("cmp x10, #2");                                       // is a complete two-byte sequence present?
    emitter.instruction("b.lo __rt_mb_strlen_utf8_invalid");                   // a truncated sequence is invalid
    emitter.instruction("ldrb w12, [x9, #1]");                                 // load the second byte
    emitter.instruction("sub w15, w12, #0x80");                                // map a continuation byte to 0..63
    emitter.instruction("cmp w15, #0x3F");                                     // is this a continuation byte?
    emitter.instruction("b.hi __rt_mb_strlen_utf8_invalid");                   // a non-continuation byte is invalid
    emitter.instruction("add x0, x0, #1");                                     // count one two-byte codepoint
    emitter.instruction("add x9, x9, #2");                                     // consume the sequence
    emitter.instruction("sub x10, x10, #2");                                   // account for the consumed bytes
    emitter.instruction("b __rt_mb_strlen_utf8_block");                        // scan the next block
    emitter.label("__rt_mb_strlen_utf8_three");
    emitter.instruction("cmp x10, #3");                                       // is a complete three-byte sequence present?
    emitter.instruction("b.lo __rt_mb_strlen_utf8_invalid");                   // a truncated sequence is invalid
    emitter.instruction("ldrb w12, [x9, #1]");                                 // load the second byte
    emitter.instruction("sub w15, w12, #0x80");                                // map a continuation byte to 0..63
    emitter.instruction("cmp w15, #0x3F");                                     // is this a continuation byte?
    emitter.instruction("b.hi __rt_mb_strlen_utf8_invalid");                   // a non-continuation byte is invalid
    emitter.instruction("cmp w11, #0xE0");                                     // does E0 constrain the second byte?
    emitter.instruction("b.ne __rt_mb_strlen_utf8_three_not_e0");             // only E0 needs the overlong check
    emitter.instruction("cmp w15, #0x20");                                     // E0 requires a second byte of at least 0xA0
    emitter.instruction("b.lo __rt_mb_strlen_utf8_invalid");                   // an overlong sequence is invalid
    emitter.label("__rt_mb_strlen_utf8_three_not_e0");
    emitter.instruction("cmp w11, #0xED");                                     // does ED constrain the second byte?
    emitter.instruction("b.ne __rt_mb_strlen_utf8_three_tail");                // only ED needs the surrogate check
    emitter.instruction("cmp w15, #0x1F");                                     // ED requires a second byte of at most 0x9F
    emitter.instruction("b.hi __rt_mb_strlen_utf8_invalid");                   // a UTF-16 surrogate is invalid
    emitter.label("__rt_mb_strlen_utf8_three_tail");
    emitter.instruction("ldrb w13, [x9, #2]");                                 // load the third byte
    emitter.instruction("sub w15, w13, #0x80");                                // map a continuation byte to 0..63
    emitter.instruction("cmp w15, #0x3F");                                     // is this a continuation byte?
    emitter.instruction("b.hi __rt_mb_strlen_utf8_invalid");                   // a non-continuation byte is invalid
    emitter.instruction("add x0, x0, #1");                                     // count one three-byte codepoint
    emitter.instruction("add x9, x9, #3");                                     // consume the sequence
    emitter.instruction("sub x10, x10, #3");                                   // account for the consumed bytes
    emitter.instruction("b __rt_mb_strlen_utf8_block");                        // scan the next block
    emitter.label("__rt_mb_strlen_utf8_invalid");
    emitter.instruction("mov x0, #-1");                                        // signal that the caller must use the bridge
    emitter.label("__rt_mb_strlen_utf8_done");
    emitter.instruction("ret");                                                // return the count or the invalid sentinel
}

/// Emits the x86_64 UTF-8 counter.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_strlen_utf8 ---");
    emitter.label_global("__rt_mb_strlen_utf8");
    emitter.instruction("mov r8, rax");                                         // subject cursor
    emitter.instruction("mov r9, rdx");                                        // remaining subject bytes
    emitter.instruction("xor eax, eax");                                       // codepoint count starts at zero
    emitter.label("__rt_mb_strlen_utf8_block_x86");
    emitter.instruction("cmp r9, 16");                                          // is a full ASCII block available?
    emitter.instruction("jb __rt_mb_strlen_utf8_scalar_x86");                  // the tail is handled byte by byte
    emitter.instruction("movdqu xmm0, XMMWORD PTR [r8]");                       // load sixteen subject bytes
    emitter.instruction("pmovmskb ecx, xmm0");                                  // collect the high bit of each byte
    emitter.instruction("test ecx, ecx");                                       // did any byte leave the ASCII range?
    emitter.instruction("jnz __rt_mb_strlen_utf8_scalar_x86");                 // non-ASCII bytes use the validator
    emitter.instruction("add rax, 16");                                         // count sixteen ASCII codepoints
    emitter.instruction("add r8, 16");                                          // consume the ASCII block
    emitter.instruction("sub r9, 16");                                          // account for the consumed block
    emitter.instruction("jmp __rt_mb_strlen_utf8_block_x86");                   // scan the next block
    emitter.label("__rt_mb_strlen_utf8_scalar_x86");
    emitter.instruction("test r9, r9");                                         // is the subject exhausted?
    emitter.instruction("jz __rt_mb_strlen_utf8_done_x86");                     // an exhausted subject is valid UTF-8
    emitter.instruction("movzx r10d, BYTE PTR [r8]");                           // load the next lead byte
    emitter.instruction("cmp r10b, 0x80");                                      // is this an ASCII byte?
    emitter.instruction("jb __rt_mb_strlen_utf8_ascii_x86");                    // count the ASCII byte directly
    emitter.instruction("cmp r10b, 0xC2");                                      // is this a valid two-byte lead?
    emitter.instruction("jb __rt_mb_strlen_utf8_invalid_x86");                 // continuation or overlong lead bytes are invalid
    emitter.instruction("cmp r10b, 0xE0");                                      // is this a two-byte lead?
    emitter.instruction("jb __rt_mb_strlen_utf8_two_x86");                     // validate the two-byte sequence
    emitter.instruction("cmp r10b, 0xF0");                                      // is this a three-byte lead?
    emitter.instruction("jb __rt_mb_strlen_utf8_three_x86");                   // validate the three-byte sequence
    emitter.instruction("cmp r10b, 0xF5");                                      // is this a valid four-byte lead?
    emitter.instruction("jae __rt_mb_strlen_utf8_invalid_x86");                // beyond U+10FFFF is invalid
    emitter.instruction("cmp r9, 4");                                          // is a complete four-byte sequence present?
    emitter.instruction("jb __rt_mb_strlen_utf8_invalid_x86");                 // a truncated sequence is invalid
    emitter.instruction("movzx r11d, BYTE PTR [r8 + 1]");                       // load the second byte
    emitter.instruction("sub r11d, 0x80");                                      // map a continuation byte to 0..63
    emitter.instruction("cmp r11d, 0x3F");                                      // is this a continuation byte?
    emitter.instruction("ja __rt_mb_strlen_utf8_invalid_x86");                 // a non-continuation byte is invalid
    emitter.instruction("cmp r10b, 0xF0");                                      // does the first byte constrain the second?
    emitter.instruction("jne __rt_mb_strlen_utf8_four_not_f0_x86");            // only U+10000..U+3FFFF uses F0
    emitter.instruction("cmp r11d, 0x10");                                      // F0 requires a second byte of at least 0x90
    emitter.instruction("jb __rt_mb_strlen_utf8_invalid_x86");                 // an overlong sequence is invalid
    emitter.label("__rt_mb_strlen_utf8_four_not_f0_x86");
    emitter.instruction("cmp r10b, 0xF4");                                      // does the first byte constrain the second?
    emitter.instruction("jne __rt_mb_strlen_utf8_four_tail_x86");              // only F4 needs the upper range check
    emitter.instruction("cmp r11d, 0x0F");                                      // F4 requires a second byte of at most 0x8F
    emitter.instruction("ja __rt_mb_strlen_utf8_invalid_x86");                 // beyond U+10FFFF is invalid
    emitter.label("__rt_mb_strlen_utf8_four_tail_x86");
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 2]");                        // load the third byte
    emitter.instruction("sub ecx, 0x80");                                       // map a continuation byte to 0..63
    emitter.instruction("cmp ecx, 0x3F");                                       // is this a continuation byte?
    emitter.instruction("ja __rt_mb_strlen_utf8_invalid_x86");                 // a non-continuation byte is invalid
    emitter.instruction("movzx edx, BYTE PTR [r8 + 3]");                        // load the fourth byte
    emitter.instruction("sub edx, 0x80");                                       // map a continuation byte to 0..63
    emitter.instruction("cmp edx, 0x3F");                                       // is this a continuation byte?
    emitter.instruction("ja __rt_mb_strlen_utf8_invalid_x86");                 // a non-continuation byte is invalid
    emitter.instruction("add rax, 1");                                          // count one four-byte codepoint
    emitter.instruction("add r8, 4");                                           // consume the sequence
    emitter.instruction("sub r9, 4");                                           // account for the consumed bytes
    emitter.instruction("jmp __rt_mb_strlen_utf8_block_x86");                  // scan the next block
    emitter.label("__rt_mb_strlen_utf8_two_x86");
    emitter.instruction("cmp r9, 2");                                           // is a complete two-byte sequence present?
    emitter.instruction("jb __rt_mb_strlen_utf8_invalid_x86");                 // a truncated sequence is invalid
    emitter.instruction("movzx r11d, BYTE PTR [r8 + 1]");                       // load the second byte
    emitter.instruction("sub r11d, 0x80");                                      // map a continuation byte to 0..63
    emitter.instruction("cmp r11d, 0x3F");                                      // is this a continuation byte?
    emitter.instruction("ja __rt_mb_strlen_utf8_invalid_x86");                 // a non-continuation byte is invalid
    emitter.instruction("add rax, 1");                                          // count one two-byte codepoint
    emitter.instruction("add r8, 2");                                           // consume the sequence
    emitter.instruction("sub r9, 2");                                           // account for the consumed bytes
    emitter.instruction("jmp __rt_mb_strlen_utf8_block_x86");                  // scan the next block
    emitter.label("__rt_mb_strlen_utf8_three_x86");
    emitter.instruction("cmp r9, 3");                                           // is a complete three-byte sequence present?
    emitter.instruction("jb __rt_mb_strlen_utf8_invalid_x86");                 // a truncated sequence is invalid
    emitter.instruction("movzx r11d, BYTE PTR [r8 + 1]");                       // load the second byte
    emitter.instruction("sub r11d, 0x80");                                      // map a continuation byte to 0..63
    emitter.instruction("cmp r11d, 0x3F");                                      // is this a continuation byte?
    emitter.instruction("ja __rt_mb_strlen_utf8_invalid_x86");                 // a non-continuation byte is invalid
    emitter.instruction("cmp r10b, 0xE0");                                      // does E0 constrain the second byte?
    emitter.instruction("jne __rt_mb_strlen_utf8_three_not_e0_x86");           // only E0 needs the overlong check
    emitter.instruction("cmp r11d, 0x20");                                      // E0 requires a second byte of at least 0xA0
    emitter.instruction("jb __rt_mb_strlen_utf8_invalid_x86");                 // an overlong sequence is invalid
    emitter.label("__rt_mb_strlen_utf8_three_not_e0_x86");
    emitter.instruction("cmp r10b, 0xED");                                      // does ED constrain the second byte?
    emitter.instruction("jne __rt_mb_strlen_utf8_three_tail_x86");             // only ED needs the surrogate check
    emitter.instruction("cmp r11d, 0x1F");                                      // ED requires a second byte of at most 0x9F
    emitter.instruction("ja __rt_mb_strlen_utf8_invalid_x86");                 // a UTF-16 surrogate is invalid
    emitter.label("__rt_mb_strlen_utf8_three_tail_x86");
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 2]");                        // load the third byte
    emitter.instruction("sub ecx, 0x80");                                       // map a continuation byte to 0..63
    emitter.instruction("cmp ecx, 0x3F");                                       // is this a continuation byte?
    emitter.instruction("ja __rt_mb_strlen_utf8_invalid_x86");                 // a non-continuation byte is invalid
    emitter.instruction("add rax, 1");                                          // count one three-byte codepoint
    emitter.instruction("add r8, 3");                                           // consume the sequence
    emitter.instruction("sub r9, 3");                                           // account for the consumed bytes
    emitter.instruction("jmp __rt_mb_strlen_utf8_block_x86");                  // scan the next block
    emitter.label("__rt_mb_strlen_utf8_ascii_x86");
    emitter.instruction("add rax, 1");                                          // count one ASCII codepoint
    emitter.instruction("add r8, 1");                                           // consume the ASCII byte
    emitter.instruction("sub r9, 1");                                           // account for the consumed byte
    emitter.instruction("jmp __rt_mb_strlen_utf8_block_x86");                  // scan the next block
    emitter.label("__rt_mb_strlen_utf8_invalid_x86");
    emitter.instruction("mov rax, -1");                                         // signal that the caller must use the bridge
    emitter.label("__rt_mb_strlen_utf8_done_x86");
    emitter.instruction("ret");                                                // return the count or the invalid sentinel
}
