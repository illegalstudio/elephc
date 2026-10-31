//! Purpose:
//! Emits the `__rt_base64_decode` runtime helper assembly, a byte-for-byte port of php-src's
//! `php_base64_decode_impl` including its `$strict` mode.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The decoder is a SINGLE-CHARACTER state machine driven by an `i % 4` accumulator, not a
//!   four-characters-at-a-time chunk loop. That distinction is the whole bug fix: a skipped
//!   byte (whitespace, or any stray byte in the lax mode) must not shift the remaining input
//!   into the wrong quartet lane, and a missing final `=` must still flush the bytes already
//!   accumulated. The old chunked loop got `base64_decode("SGVs bG8=")`, `"SGVsbG8"`, and
//!   `"SGVsbG8*"` all wrong.
//! - `_b64_decode_tbl` classifies every byte in one load: `0..=63` is a sextet,
//!   `B64_DECODE_SKIP` is php-src's `-1` (whitespace, dropped in both modes), and
//!   `B64_DECODE_INVALID` is its `-2` (dropped in the lax mode, `false` in strict mode).
//!   `=` is recognized before the lookup, exactly as php-src does.
//! - Result storage comes from `__rt_concat_reserve`/`__rt_concat_publish`, so an input above
//!   the 64 KiB scratch capacity is served from an owned heap block. A strict rejection
//!   releases that reservation through `__rt_heap_free_safe` (a no-op for scratch pointers)
//!   instead of leaking it.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Reverse-table sentinel for php-src's `-1`: a byte that is skipped in BOTH decode modes.
///
/// php-src assigns it to exactly the five whitespace bytes listed in
/// [`B64_DECODE_WHITESPACE`]; every other non-alphabet byte is [`B64_DECODE_INVALID`].
pub const B64_DECODE_SKIP: u8 = 0xFE;

/// Reverse-table sentinel for php-src's `-2`: a byte outside the Base64 alphabet.
///
/// The lax mode drops it and keeps decoding; `$strict = true` returns `false` on the first
/// one. `=` also carries this value, but the decoder tests for it before the table load
/// because padding has its own accounting.
pub const B64_DECODE_INVALID: u8 = 0xFF;

/// The exact byte set php-src marks skippable in `base64_reverse_table`.
///
/// Tab, line feed, form feed, carriage return, and space — deliberately NOT vertical tab
/// (`0x0B`), which php-src rejects and `u8::is_ascii_whitespace` would have accepted.
pub const B64_DECODE_WHITESPACE: &[u8] = &[b'\t', b'\n', 0x0C, b'\r', b' '];

/// Emits the `__rt_base64_decode` runtime helper.
///
/// ABI (AArch64): `x1` = encoded pointer, `x2` = encoded byte length, `x3` = `$strict` flag.
/// Returns `x1`/`x2` = decoded pointer/length and `x0` = 1 on success, 0 when strict mode
/// rejected the input (in which case `x1`/`x2` are a null/empty pair).
///
/// Dispatches to `emit_base64_decode_linux_x86_64` on x86_64; uses inline AArch64 otherwise.
pub fn emit_base64_decode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_base64_decode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: base64_decode ---");
    emitter.label_global("__rt_base64_decode");

    // -- reserve the decoded result up front: 3 bytes out per 4 in, so the encoded length is
    //    always an upper bound, and floor(3n/4) <= n-1 keeps every partial-byte store in range --
    emitter.instruction("sub sp, sp, #48");                                     // allocate spill space for the borrowed encoded string and the strict flag
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across the reservation call
    emitter.instruction("add x29, sp, #32");                                    // establish the base64 decoder frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save the encoded pointer and length across the reservation call
    emitter.instruction("str x3, [sp, #16]");                                   // save the $strict flag across the reservation call
    emitter.instruction("mov x0, x2");                                          // the decoded payload never exceeds the encoded character count
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the decoded result
    emitter.instruction("mov x9, x0");                                          // keep the reservation start as the decoded string base
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload the borrowed encoded pointer and length
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the $strict flag for the per-character decisions
    emitter.instruction("mov x10, #0");                                         // j: index of the decoded byte currently being assembled
    emitter.instruction("mov x11, #0");                                         // i: count of ACCEPTED characters, the `i % 4` quartet lane
    emitter.instruction("mov x12, #0");                                         // padding: '=' characters seen since the last accepted character
    emitter.instruction("mov x16, #0");                                         // accumulated sextets not yet written as bytes
    emitter.instruction("mov x17, #0");                                         // number of pending bits in the accumulator
    abi::emit_symbol_address(emitter, "x15", "_b64_decode_tbl");
    emitter.instruction("b __rt_b64dec_fast");                                  // start with the four-character fast path

    // -- four-character fast path: all four bytes must be alphabet sextets --
    emitter.label("__rt_b64dec_fast");
    emitter.instruction("cmp x2, #4");                                          // is a full quartet available?
    emitter.instruction("b.lt __rt_b64dec_loop");                               // the tail keeps the one-character state machine
    emitter.instruction("ldrb w4, [x1]");                                       // load the first encoded byte
    emitter.instruction("ldrb w4, [x15, x4]");                                  // classify the first byte
    emitter.instruction("cmp w4, #64");                                         // is it a Base64 sextet?
    emitter.instruction("b.hs __rt_b64dec_loop");                               // padding, whitespace or invalid input falls back
    emitter.instruction("ldrb w5, [x1, #1]");                                   // load the second encoded byte
    emitter.instruction("ldrb w5, [x15, x5]");                                  // classify the second byte
    emitter.instruction("cmp w5, #64");                                         // is it a Base64 sextet?
    emitter.instruction("b.hs __rt_b64dec_loop");                               // fall back on any non-sextet
    emitter.instruction("ldrb w6, [x1, #2]");                                   // load the third encoded byte
    emitter.instruction("ldrb w6, [x15, x6]");                                  // classify the third byte
    emitter.instruction("cmp w6, #64");                                         // is it a Base64 sextet?
    emitter.instruction("b.hs __rt_b64dec_loop");                               // fall back on any non-sextet
    emitter.instruction("ldrb w7, [x1, #3]");                                   // load the fourth encoded byte
    emitter.instruction("ldrb w7, [x15, x7]");                                  // classify the fourth byte
    emitter.instruction("cmp w7, #64");                                         // is it a Base64 sextet?
    emitter.instruction("b.hs __rt_b64dec_loop");                               // fall back on any non-sextet

    // -- decode the four sextets into three bytes --
    emitter.instruction("lsl w8, w4, #2");                                      // first byte = top six bits of sextet 0
    emitter.instruction("lsr w0, w5, #4");                                      // plus the top two bits of sextet 1
    emitter.instruction("orr w8, w8, w0");                                      // assemble the first decoded byte
    emitter.instruction("strb w8, [x9, x10]");                                  // write the first decoded byte
    emitter.instruction("add x14, x9, x10");                                    // address the second decoded byte
    emitter.instruction("lsl w8, w5, #4");                                      // second byte = low four bits of sextet 1
    emitter.instruction("lsr w0, w6, #2");                                      // plus the top four bits of sextet 2
    emitter.instruction("orr w8, w8, w0");                                      // assemble the second decoded byte
    emitter.instruction("strb w8, [x14, #1]");                                  // write the second decoded byte
    emitter.instruction("lsl w8, w6, #6");                                      // third byte = low two bits of sextet 2
    emitter.instruction("orr w8, w8, w7");                                      // plus all six bits of sextet 3
    emitter.instruction("strb w8, [x14, #2]");                                  // write the third decoded byte
    emitter.instruction("add x1, x1, #4");                                      // consume the four encoded bytes
    emitter.instruction("sub x2, x2, #4");                                      // account for the consumed quartet
    emitter.instruction("add x10, x10, #3");                                    // account for the three decoded bytes
    emitter.instruction("add x11, x11, #4");                                    // four accepted characters are now accounted for
    emitter.instruction("b __rt_b64dec_fast");                                  // process the next complete quartet

    // -- one character per iteration: skipped bytes must not rotate the quartet lane --
    emitter.label("__rt_b64dec_loop");
    emitter.instruction("cbz x2, __rt_b64dec_end");                             // stop once every encoded byte has been classified
    emitter.instruction("ldrb w13, [x1], #1");                                  // load the next encoded byte and advance the cursor
    emitter.instruction("sub x2, x2, #1");                                      // record that one encoded byte has been consumed
    emitter.instruction("cmp w13, #61");                                        // is this byte '=' padding?
    emitter.instruction("b.eq __rt_b64dec_pad");                                // padding is counted, never decoded
    emitter.instruction("ldrb w13, [x15, x13]");                                // classify the byte through the php-src reverse table
    emitter.instruction(&format!("cmp w13, #{}", B64_DECODE_SKIP));             // is this one of php-src's five skippable whitespace bytes?
    emitter.instruction("b.eq __rt_b64dec_loop");                               // whitespace is dropped in both decode modes
    emitter.instruction(&format!("cmp w13, #{}", B64_DECODE_INVALID));          // is this byte outside the Base64 alphabet?
    emitter.instruction("b.eq __rt_b64dec_invalid");                            // strict mode rejects it; the lax mode drops it

    // -- an accepted character after padding ends the message in strict mode --
    emitter.instruction("cbz x12, __rt_b64dec_accept");                         // no padding seen, so the character is accepted directly
    emitter.instruction("cbnz x3, __rt_b64dec_fail");                           // strict mode forbids data after a padding character
    emitter.instruction("mov x12, #0");                                         // the lax mode forgets the padding and keeps decoding

    // -- append the sextet and flush every complete decoded byte --
    emitter.label("__rt_b64dec_accept");
    emitter.instruction("lsl w16, w16, #6");                                    // make room for the next six input bits
    emitter.instruction("orr w16, w16, w13");                                   // append this sextet to the pending accumulator
    emitter.instruction("add w17, w17, #6");                                    // six more bits are pending
    emitter.instruction("cmp w17, #8");                                         // is a full decoded byte available?
    emitter.instruction("b.lt __rt_b64dec_next");                               // fewer than eight bits still wait for more input
    emitter.instruction("sub w17, w17, #8");                                    // consume the eight bits of the next output byte
    emitter.instruction("lsr w14, w16, w17");                                   // shift the completed byte into the low bits
    emitter.instruction("strb w14, [x9, x10]");                                 // publish the decoded byte
    emitter.instruction("add x10, x10, #1");                                    // advance the decoded byte cursor
    emitter.instruction("and w16, w16, #0x3f");                                 // keep only the bits that still belong to the next byte

    emitter.label("__rt_b64dec_next");
    emitter.instruction("add x11, x11, #1");                                    // one more accepted character advances the strict-mode accounting
    emitter.instruction("b __rt_b64dec_loop");                                  // classify the next encoded byte

    // -- '=' only ever increments the padding tally --
    emitter.label("__rt_b64dec_pad");
    emitter.instruction("add x12, x12, #1");                                    // count this padding character for the strict-mode checks
    emitter.instruction("b __rt_b64dec_loop");                                  // classify the next encoded byte

    // -- a byte outside the alphabet: rejected in strict mode, dropped otherwise --
    emitter.label("__rt_b64dec_invalid");
    emitter.instruction("cbnz x3, __rt_b64dec_fail");                           // strict mode returns false on the first stray byte
    emitter.instruction("b __rt_b64dec_loop");                                  // the lax mode ignores it and keeps decoding

    // -- strict-mode end-of-input validation, in php-src's order --
    emitter.label("__rt_b64dec_end");
    emitter.instruction("cbz x3, __rt_b64dec_done");                            // the lax mode accepts whatever was decoded
    emitter.instruction("and x14, x11, #3");                                    // recover the quartet lane the input ended on
    emitter.instruction("cmp x14, #1");                                         // did the final group hold a single character?
    emitter.instruction("b.eq __rt_b64dec_fail");                               // one leftover character cannot encode any byte
    emitter.instruction("cbz x12, __rt_b64dec_done");                           // unpadded input is accepted when the group is not truncated
    emitter.instruction("cmp x12, #2");                                         // more than two padding characters is never valid
    emitter.instruction("b.gt __rt_b64dec_fail");                               // reject over-padded input such as "A==="
    emitter.instruction("add x14, x11, x12");                                   // characters plus padding must complete whole quartets
    emitter.instruction("and x14, x14, #3");                                    // check that combined count against the quartet size
    emitter.instruction("cbnz x14, __rt_b64dec_fail");                          // reject misplaced padding such as "SGVsbG8=="

    emitter.label("__rt_b64dec_done");
    emitter.instruction("mov x1, x9");                                          // return the decoded payload pointer
    emitter.instruction("mov x2, x10");                                         // return the number of decoded bytes actually written
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("mov x0, #1");                                          // report a successful decode to the caller
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the base64 decoder frame
    emitter.instruction("ret");                                                 // return the decoded string pair

    // -- strict rejection: release the reservation and report PHP's `false` --
    emitter.label("__rt_b64dec_fail");
    emitter.instruction("mov x0, x9");                                          // release the reservation that will never be published
    emitter.instruction("bl __rt_heap_free_safe");                              // free an oversized heap reservation; scratch pointers are skipped
    emitter.instruction("mov x0, #0");                                          // report the strict-mode rejection to the caller
    emitter.instruction("mov x1, #0");                                          // hand back a null payload pointer with the failure
    emitter.instruction("mov x2, #0");                                          // hand back a zero payload length with the failure
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the base64 decoder frame
    emitter.instruction("ret");                                                 // return PHP's `false` decode result
}

/// Emits the `__rt_base64_decode` runtime helper for the Linux x86_64 target.
///
/// ABI (x86_64): `rax` = encoded pointer, `rdx` = encoded byte length, `rdi` = `$strict` flag.
/// Returns `rax`/`rdx` = decoded pointer/length and `r8` = 1 on success, 0 when strict mode
/// rejected the input.
///
/// Same single-character state machine as the AArch64 path; the padding tally lives in a
/// frame slot because the alphabet table, the input cursor, the output cursor, the accepted
/// count, and the strict flag already occupy the free caller-saved registers.
/// Called exclusively from `emit_base64_decode` when `emitter.target.arch == Arch::X86_64`.
fn emit_base64_decode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: base64_decode ---");
    emitter.label_global("__rt_base64_decode");

    emitter.instruction("push r14");                                            // preserve the fast-path sextet registers
    emitter.instruction("push r15");                                            // preserve the second fast-path sextet register
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the reservation and publish calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the borrowed encoded string
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the input, the strict flag, and the padding tally
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the encoded string pointer across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the encoded character count across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the $strict flag across the reservation call
    emitter.instruction("mov rax, rdx");                                        // the decoded payload never exceeds the encoded character count
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the decoded result
    emitter.instruction("mov r9, rax");                                         // keep the reservation start as the decoded string base
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // padding: '=' characters seen since the last accepted character
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the encoded string pointer into the read cursor
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the encoded character count into the loop counter
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the $strict flag for the per-character decisions
    emitter.instruction("xor r10d, r10d");                                      // j: index of the decoded byte currently being assembled
    emitter.instruction("xor r11d, r11d");                                      // i: count of ACCEPTED characters, the `i % 4` quartet lane
    abi::emit_symbol_address(emitter, "r8", "_b64_decode_tbl");                 // hold the php-src reverse-lookup table for the whole decode loop
    emitter.instruction("jmp __rt_b64dec_fast_x86");                            // start with the four-character fast path

    // -- four-character fast path: all four bytes must be alphabet sextets --
    emitter.label("__rt_b64dec_fast_x86");
    emitter.instruction("cmp rcx, 4");                                          // is a full quartet available?
    emitter.instruction("jb __rt_b64dec_loop_x86");                             // the tail keeps the one-character state machine
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the first encoded byte
    emitter.instruction("movzx eax, BYTE PTR [r8 + rax]");                      // classify the first byte
    emitter.instruction("cmp eax, 64");                                         // is it a Base64 sextet?
    emitter.instruction("jae __rt_b64dec_loop_x86");                            // padding, whitespace or invalid input falls back
    emitter.instruction("movzx edx, BYTE PTR [rsi + 1]");                       // load the second encoded byte
    emitter.instruction("movzx edx, BYTE PTR [r8 + rdx]");                      // classify the second byte
    emitter.instruction("cmp edx, 64");                                         // is it a Base64 sextet?
    emitter.instruction("jae __rt_b64dec_loop_x86");                            // fall back on any non-sextet
    emitter.instruction("movzx r14d, BYTE PTR [rsi + 2]");                      // load the third encoded byte
    emitter.instruction("movzx r14d, BYTE PTR [r8 + r14]");                     // classify the third byte
    emitter.instruction("cmp r14d, 64");                                        // is it a Base64 sextet?
    emitter.instruction("jae __rt_b64dec_loop_x86");                            // fall back on any non-sextet
    emitter.instruction("movzx r15d, BYTE PTR [rsi + 3]");                      // load the fourth encoded byte
    emitter.instruction("movzx r15d, BYTE PTR [r8 + r15]");                     // classify the fourth byte
    emitter.instruction("cmp r15d, 64");                                        // is it a Base64 sextet?
    emitter.instruction("jae __rt_b64dec_loop_x86");                            // fall back on any non-sextet

    // -- decode the four sextets into three bytes --
    emitter.instruction("shl eax, 2");                                          // first byte = top six bits of sextet 0
    emitter.instruction("mov r8d, edx");                                        // copy sextet 1 before extracting its high bits
    emitter.instruction("shr r8d, 4");                                          // plus the top two bits of sextet 1
    emitter.instruction("or eax, r8d");                                         // assemble the first decoded byte
    emitter.instruction("mov BYTE PTR [r9 + r10], al");                         // write the first decoded byte
    emitter.instruction("shl edx, 4");                                          // second byte = low four bits of sextet 1
    emitter.instruction("mov r8d, r14d");                                       // copy sextet 2 before extracting its high bits
    emitter.instruction("shr r8d, 2");                                          // plus the top four bits of sextet 2
    emitter.instruction("or edx, r8d");                                         // assemble the second decoded byte
    emitter.instruction("mov BYTE PTR [r9 + r10 + 1], dl");                     // write the second decoded byte
    emitter.instruction("shl r14d, 6");                                         // third byte = low two bits of sextet 2
    emitter.instruction("or r14d, r15d");                                       // plus all six bits of sextet 3
    emitter.instruction("mov BYTE PTR [r9 + r10 + 2], r14b");                   // write the third decoded byte
    emitter.instruction("add rsi, 4");                                          // consume the four encoded bytes
    emitter.instruction("sub rcx, 4");                                          // account for the consumed quartet
    emitter.instruction("add r10, 3");                                          // account for the three decoded bytes
    emitter.instruction("add r11, 4");                                          // four accepted characters are now accounted for
    abi::emit_symbol_address(emitter, "r8", "_b64_decode_tbl");                 // restore the reverse table for the next quartet
    emitter.instruction("jmp __rt_b64dec_fast_x86");                            // process the next complete quartet

    emitter.label("__rt_b64dec_loop_x86");
    emitter.instruction("test rcx, rcx");                                       // stop once every encoded byte has been classified
    emitter.instruction("jz __rt_b64dec_end_x86");                              // leave the loop at the end of the encoded input
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the next encoded byte and widen it for the table lookup
    emitter.instruction("add rsi, 1");                                          // advance the encoded-string read cursor
    emitter.instruction("sub rcx, 1");                                          // record that one encoded byte has been consumed
    emitter.instruction("cmp eax, 61");                                         // is this byte '=' padding?
    emitter.instruction("je __rt_b64dec_pad_x86");                              // padding is counted, never decoded
    emitter.instruction("movzx eax, BYTE PTR [r8 + rax]");                      // classify the byte through the php-src reverse table
    emitter.instruction(&format!("cmp eax, {}", B64_DECODE_SKIP));              // is this one of php-src's five skippable whitespace bytes?
    emitter.instruction("je __rt_b64dec_loop_x86");                             // whitespace is dropped in both decode modes
    emitter.instruction(&format!("cmp eax, {}", B64_DECODE_INVALID));           // is this byte outside the Base64 alphabet?
    emitter.instruction("je __rt_b64dec_invalid_x86");                          // strict mode rejects it; the lax mode drops it

    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // has any padding character been seen already?
    emitter.instruction("je __rt_b64dec_accept_x86");                           // no padding seen, so the character is accepted directly
    emitter.instruction("test rdi, rdi");                                       // is this a strict decode?
    emitter.instruction("jnz __rt_b64dec_fail_x86");                            // strict mode forbids data after a padding character
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // the lax mode forgets the padding and keeps decoding

    emitter.label("__rt_b64dec_accept_x86");
    emitter.instruction("mov rdx, r11");                                        // copy the accepted-character count before reducing it
    emitter.instruction("and rdx, 3");                                          // compute the quartet lane of this accepted character
    emitter.instruction("jz __rt_b64dec_case0_x86");                            // lane 0 starts a new decoded byte
    emitter.instruction("cmp rdx, 1");                                          // is this the second character of the quartet?
    emitter.instruction("je __rt_b64dec_case1_x86");                            // lane 1 finishes byte 0 and opens byte 1
    emitter.instruction("cmp rdx, 2");                                          // is this the third character of the quartet?
    emitter.instruction("je __rt_b64dec_case2_x86");                            // lane 2 finishes byte 1 and opens byte 2

    emitter.instruction("or BYTE PTR [r9 + r10], al");                          // fold all six bits of this sextet into the byte opened by lane 2
    emitter.instruction("add r10, 1");                                          // the quartet produced its third and final byte
    emitter.instruction("jmp __rt_b64dec_next_x86");                            // count the accepted character and continue

    emitter.label("__rt_b64dec_case0_x86");
    emitter.instruction("shl eax, 2");                                          // shift the sextet into the high bits of the new byte
    emitter.instruction("mov BYTE PTR [r9 + r10], al");                         // open the decoded byte without committing it yet
    emitter.instruction("jmp __rt_b64dec_next_x86");                            // count the accepted character and continue

    emitter.label("__rt_b64dec_case1_x86");
    emitter.instruction("mov edx, eax");                                        // copy the sextet before splitting it across two decoded bytes
    emitter.instruction("shr edx, 4");                                          // take the top two bits of this sextet
    emitter.instruction("or BYTE PTR [r9 + r10], dl");                          // complete the first decoded byte of the quartet
    emitter.instruction("add r10, 1");                                          // move to the next decoded byte position
    emitter.instruction("and eax, 15");                                         // keep the low four bits of this sextet
    emitter.instruction("shl eax, 4");                                          // shift them into the high bits of the next byte
    emitter.instruction("mov BYTE PTR [r9 + r10], al");                         // open the second decoded byte of the quartet
    emitter.instruction("jmp __rt_b64dec_next_x86");                            // count the accepted character and continue

    emitter.label("__rt_b64dec_case2_x86");
    emitter.instruction("mov edx, eax");                                        // copy the sextet before splitting it across two decoded bytes
    emitter.instruction("shr edx, 2");                                          // take the top four bits of this sextet
    emitter.instruction("or BYTE PTR [r9 + r10], dl");                          // complete the second decoded byte of the quartet
    emitter.instruction("add r10, 1");                                          // move to the next decoded byte position
    emitter.instruction("and eax, 3");                                          // keep the low two bits of this sextet
    emitter.instruction("shl eax, 6");                                          // shift them into the high bits of the next byte
    emitter.instruction("mov BYTE PTR [r9 + r10], al");                         // open the third decoded byte of the quartet

    emitter.label("__rt_b64dec_next_x86");
    emitter.instruction("add r11, 1");                                          // one more accepted character advances the quartet lane
    emitter.instruction("jmp __rt_b64dec_loop_x86");                            // classify the next encoded byte

    emitter.label("__rt_b64dec_pad_x86");
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // count this padding character for the strict-mode checks
    emitter.instruction("jmp __rt_b64dec_loop_x86");                            // classify the next encoded byte

    emitter.label("__rt_b64dec_invalid_x86");
    emitter.instruction("test rdi, rdi");                                       // is this a strict decode?
    emitter.instruction("jnz __rt_b64dec_fail_x86");                            // strict mode returns false on the first stray byte
    emitter.instruction("jmp __rt_b64dec_loop_x86");                            // the lax mode ignores it and keeps decoding

    emitter.label("__rt_b64dec_end_x86");
    emitter.instruction("test rdi, rdi");                                       // is this a strict decode?
    emitter.instruction("jz __rt_b64dec_done_x86");                             // the lax mode accepts whatever was decoded
    emitter.instruction("mov rdx, r11");                                        // copy the accepted-character count before reducing it
    emitter.instruction("and rdx, 3");                                          // recover the quartet lane the input ended on
    emitter.instruction("cmp rdx, 1");                                          // did the final group hold a single character?
    emitter.instruction("je __rt_b64dec_fail_x86");                             // one leftover character cannot encode any byte
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the padding tally for the remaining checks
    emitter.instruction("test rdx, rdx");                                       // was the input padded at all?
    emitter.instruction("jz __rt_b64dec_done_x86");                             // unpadded input is accepted when the group is not truncated
    emitter.instruction("cmp rdx, 2");                                          // more than two padding characters is never valid
    emitter.instruction("jg __rt_b64dec_fail_x86");                             // reject over-padded input such as "A==="
    emitter.instruction("add rdx, r11");                                        // characters plus padding must complete whole quartets
    emitter.instruction("and rdx, 3");                                          // check that combined count against the quartet size
    emitter.instruction("jnz __rt_b64dec_fail_x86");                            // reject misplaced padding such as "SGVsbG8=="

    emitter.label("__rt_b64dec_done_x86");
    emitter.instruction("mov rax, r9");                                         // return the decoded payload pointer
    emitter.instruction("mov rdx, r10");                                        // return the number of decoded bytes actually written
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("mov r8, 1");                                           // report a successful decode to the caller
    emitter.instruction("add rsp, 64");                                         // release the base64 decoder spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("pop r15");                                             // restore the second fast-path sextet register
    emitter.instruction("pop r14");                                             // restore the first fast-path sextet register
    emitter.instruction("ret");                                                 // return the decoded string pair

    emitter.label("__rt_b64dec_fail_x86");
    emitter.instruction("mov rax, r9");                                         // release the reservation that will never be published
    emitter.instruction("call __rt_heap_free_safe");                            // free an oversized heap reservation; scratch pointers are skipped
    emitter.instruction("xor eax, eax");                                        // hand back a null payload pointer with the failure
    emitter.instruction("xor edx, edx");                                        // hand back a zero payload length with the failure
    emitter.instruction("xor r8d, r8d");                                        // report the strict-mode rejection to the caller
    emitter.instruction("add rsp, 64");                                         // release the base64 decoder spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("pop r15");                                             // restore the second fast-path sextet register
    emitter.instruction("pop r14");                                             // restore the first fast-path sextet register
    emitter.instruction("ret");                                                 // return PHP's `false` decode result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Verifies both targets keep the four-character decode fast path.
    #[test]
    fn base64_decode_emits_the_quartet_fast_path_on_all_targets() {
        for (name, label) in [
            ("macos-aarch64", "__rt_b64dec_fast"),
            ("linux-x86_64", "__rt_b64dec_fast_x86"),
        ] {
            let target = Target::parse(name).expect("supported target");
            let mut emitter = Emitter::new(target);
            emit_base64_decode(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains(label), "{name}: {assembly}");
        }
    }
}
