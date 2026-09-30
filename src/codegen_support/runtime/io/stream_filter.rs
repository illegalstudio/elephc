//! Purpose:
//! Emits the `__rt_apply_stream_filter` runtime helper, which applies a
//! built-in stream filter to a buffer in place.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_fread` (read direction) and the `fwrite` emitter (write direction).
//!
//! Key details:
//! - Filter ids: 1 = `string.toupper`, 2 = `string.tolower`, 3 = `string.rot13`.
//!   All three are 1:1 byte transforms, so the buffer length never changes.
//! - No arm claims id 13 (`consumed`): php's `consumed` filter appends every
//!   bucket to its output brigade unchanged and only counts bytes (php-src
//!   ext/standard/filters.c:1649-1653), so the unknown-id fallthrough — leave
//!   the byte alone — *is* its transform.
//! - `string.strip_tags` has no id: php removed the filter in 8.0, so an attach
//!   must miss and report `Unable to locate filter` instead.
//! - This is a leaf helper: it transforms the buffer in place and preserves the
//!   pointer/length registers so callers can return them unchanged.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::abi;
use crate::codegen_support::runtime::resources::layout::{
    STREAM_MODE_LEN_OFFSET, STREAM_MODE_PTR_OFFSET,
};

/// stream_filter_default_mode: php's `$mode = 0` filter-direction default.
///
/// php does not default to both chains — it reads the stream's OWN open mode and
/// enables the chains that mode can use (php-src streamsfuncs.c:1202-1214):
///   `r` or `+` → `STREAM_FILTER_READ`, `w`, `a` or `+` → `STREAM_FILTER_WRITE`.
/// The recorded mode string is php's own input, so the rule is transcribed
/// literally rather than reconstructed from the descriptor's access bits, which
/// collapse `a` into `w` and lose the distinction php tests.
///
/// A handle whose state has gone (a closed stream) answers `STREAM_FILTER_ALL`,
/// which is what every attach used before this default existed. A mode naming no
/// direction at all (`x`, `c`) answers 0, and the node then joins no chain —
/// php refuses the attach outright in that case and returns false, a remaining
/// gap recorded on `materialize_stream_filter_mode`.
///
/// Input:  AArch64 x0 = stream handle. x86_64 rdi = stream handle.
/// Output: 1 = READ, 2 = WRITE, 3 = ALL, 0 = neither.
pub fn emit_stream_filter_default_mode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_filter_default_mode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_filter_default_mode ---");
    emitter.label_global("__rt_stream_filter_default_mode");
    emitter.instruction("sub sp, sp, #16");                                     // frame for the saved linkage
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_sfdm_all");                               // no live stream: keep both chains
    emitter.instruction(&format!("ldr x1, [x0, #{STREAM_MODE_PTR_OFFSET}]"));   // the mode string php recorded at open
    emitter.instruction(&format!("ldr x2, [x0, #{STREAM_MODE_LEN_OFFSET}]"));   // its byte length
    emitter.instruction("cbz x1, __rt_sfdm_all");                               // no recorded mode: keep both chains
    emitter.instruction("mov x3, #0");                                          // accumulated direction bits
    emitter.instruction("mov x4, #0");                                          // scan cursor
    emitter.label("__rt_sfdm_scan");
    emitter.instruction("cmp x4, x2");                                          // scanned every byte?
    emitter.instruction("b.ge __rt_sfdm_done");                                 // the mode is fully classified
    emitter.instruction("ldrb w5, [x1, x4]");                                   // the next mode byte
    emitter.instruction("add x4, x4, #1");                                      // advance the cursor
    emitter.instruction("cmp w5, #0x72");                                       // 'r' — php enables the read chain
    emitter.instruction("b.eq __rt_sfdm_read");
    emitter.instruction("cmp w5, #0x2B");                                       // '+' — php enables both chains
    emitter.instruction("b.eq __rt_sfdm_both");
    emitter.instruction("cmp w5, #0x77");                                       // 'w' — php enables the write chain
    emitter.instruction("b.eq __rt_sfdm_write");
    emitter.instruction("cmp w5, #0x61");                                       // 'a' — php enables the write chain
    emitter.instruction("b.eq __rt_sfdm_write");
    emitter.instruction("b __rt_sfdm_scan");                                    // `b`, `t`, `x`, `c` name no direction
    emitter.label("__rt_sfdm_read");
    emitter.instruction("orr x3, x3, #1");                                      // STREAM_FILTER_READ
    emitter.instruction("b __rt_sfdm_scan");
    emitter.label("__rt_sfdm_write");
    emitter.instruction("orr x3, x3, #2");                                      // STREAM_FILTER_WRITE
    emitter.instruction("b __rt_sfdm_scan");
    emitter.label("__rt_sfdm_both");
    emitter.instruction("orr x3, x3, #3");                                      // STREAM_FILTER_ALL
    emitter.instruction("b __rt_sfdm_scan");
    emitter.label("__rt_sfdm_done");
    emitter.instruction("mov x0, x3");                                          // return the deduced direction bits
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return to the attach lowering
    emitter.label("__rt_sfdm_all");
    emitter.instruction("mov x0, #3");                                          // STREAM_FILTER_ALL
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return to the attach lowering
}

/// Emits the Linux x86_64 stream runtime helper for stream filter default mode.
fn emit_stream_filter_default_mode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_filter_default_mode ---");
    emitter.label_global("__rt_stream_filter_default_mode");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");                                       // no live stream?
    emitter.instruction("jz __rt_sfdm_all_x86");                                // keep both chains
    emitter.instruction(&format!("mov rsi, QWORD PTR [rax + {STREAM_MODE_PTR_OFFSET}]")); // the mode string php recorded at open
    emitter.instruction(&format!("mov rdx, QWORD PTR [rax + {STREAM_MODE_LEN_OFFSET}]")); // its byte length
    emitter.instruction("test rsi, rsi");                                       // no recorded mode?
    emitter.instruction("jz __rt_sfdm_all_x86");                                // keep both chains
    emitter.instruction("xor r8, r8");                                          // accumulated direction bits
    emitter.instruction("xor r9, r9");                                          // scan cursor
    emitter.label("__rt_sfdm_scan_x86");
    emitter.instruction("cmp r9, rdx");                                         // scanned every byte?
    emitter.instruction("jge __rt_sfdm_done_x86");                              // the mode is fully classified
    emitter.instruction("movzx ecx, BYTE PTR [rsi + r9]");                      // the next mode byte
    emitter.instruction("add r9, 1");                                           // advance the cursor
    emitter.instruction("cmp cl, 0x72");                                        // 'r' — php enables the read chain
    emitter.instruction("je __rt_sfdm_read_x86");
    emitter.instruction("cmp cl, 0x2B");                                        // '+' — php enables both chains
    emitter.instruction("je __rt_sfdm_both_x86");
    emitter.instruction("cmp cl, 0x77");                                        // 'w' — php enables the write chain
    emitter.instruction("je __rt_sfdm_write_x86");
    emitter.instruction("cmp cl, 0x61");                                        // 'a' — php enables the write chain
    emitter.instruction("je __rt_sfdm_write_x86");
    emitter.instruction("jmp __rt_sfdm_scan_x86");                              // `b`, `t`, `x`, `c` name no direction
    emitter.label("__rt_sfdm_read_x86");
    emitter.instruction("or r8, 1");                                            // STREAM_FILTER_READ
    emitter.instruction("jmp __rt_sfdm_scan_x86");
    emitter.label("__rt_sfdm_write_x86");
    emitter.instruction("or r8, 2");                                            // STREAM_FILTER_WRITE
    emitter.instruction("jmp __rt_sfdm_scan_x86");
    emitter.label("__rt_sfdm_both_x86");
    emitter.instruction("or r8, 3");                                            // STREAM_FILTER_ALL
    emitter.instruction("jmp __rt_sfdm_scan_x86");
    emitter.label("__rt_sfdm_done_x86");
    emitter.instruction("mov rax, r8");                                         // return the deduced direction bits
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the attach lowering
    emitter.label("__rt_sfdm_all_x86");
    emitter.instruction("mov eax, 3");                                          // STREAM_FILTER_ALL
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the attach lowering
}

/// apply_stream_filter: transform a buffer in place with a built-in filter.
/// Input:  AArch64 x1 = pointer, x2 = length, x3 = filter id
///         x86_64  rax = pointer, rdx = length, rcx = filter id
/// Output: the buffer is transformed in place; the pointer/length are preserved.
pub fn emit_apply_stream_filter(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_apply_stream_filter_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: apply_stream_filter ---");
    emitter.label_global("__rt_apply_stream_filter");

    emitter.instruction("mov x9, #0");                                          // x9 = current byte index
    emitter.label("__rt_asf_loop");
    emitter.instruction("cmp x9, x2");                                          // processed every byte?
    emitter.instruction("b.ge __rt_asf_done");                                  // stop when the buffer is exhausted
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the current byte
    emitter.instruction("cmp x3, #1");                                          // filter id 1 = string.toupper
    emitter.instruction("b.eq __rt_asf_upper");                                 // dispatch to the uppercase transform
    emitter.instruction("cmp x3, #2");                                          // filter id 2 = string.tolower
    emitter.instruction("b.eq __rt_asf_lower");                                 // dispatch to the lowercase transform
    emitter.instruction("cmp x3, #3");                                          // filter id 3 = string.rot13
    emitter.instruction("b.eq __rt_asf_rot13");                                 // dispatch to the rot13 transform
    emitter.instruction("cmp x3, #7");                                          // filter id 7 = convert.base64-decode
    emitter.instruction("b.eq __rt_asf_b64_decode");                            // dispatch to the base64-decode state machine
    emitter.instruction("cmp x3, #5");                                          // filter id 5 = dechunk
    emitter.instruction("b.eq __rt_asf_dechunk");                               // dispatch to the HTTP/1.1 chunked-encoding parser
    emitter.instruction("cmp x3, #6");                                          // filter id 6 = convert.base64-encode
    emitter.instruction("b.eq __rt_asf_b64_encode");                            // dispatch to the base64-encode helper
    emitter.instruction("cmp x3, #9");                                          // filter id 9 = convert.quoted-printable-decode
    emitter.instruction("b.eq __rt_asf_qp_decode");                             // dispatch to the QP decoder
    emitter.instruction("cmp x3, #8");                                          // filter id 8 = convert.quoted-printable-encode
    emitter.instruction("b.eq __rt_asf_qp_encode");                             // dispatch to the QP encoder
    emitter.instruction("b __rt_asf_next");                                     // unknown id: leave the byte unchanged

    emitter.label("__rt_asf_upper");
    emitter.instruction("cmp w10, #0x61");                                      // below 'a'?
    emitter.instruction("b.lt __rt_asf_next");                                  // non-letter: leave unchanged
    emitter.instruction("cmp w10, #0x7A");                                      // above 'z'?
    emitter.instruction("b.gt __rt_asf_next");                                  // non-letter: leave unchanged
    emitter.instruction("sub w10, w10, #0x20");                                 // lowercase -> uppercase
    emitter.instruction("b __rt_asf_store");                                    // store the transformed byte

    emitter.label("__rt_asf_lower");
    emitter.instruction("cmp w10, #0x41");                                      // below 'A'?
    emitter.instruction("b.lt __rt_asf_next");                                  // non-letter: leave unchanged
    emitter.instruction("cmp w10, #0x5A");                                      // above 'Z'?
    emitter.instruction("b.gt __rt_asf_next");                                  // non-letter: leave unchanged
    emitter.instruction("add w10, w10, #0x20");                                 // uppercase -> lowercase
    emitter.instruction("b __rt_asf_store");                                    // store the transformed byte

    emitter.label("__rt_asf_rot13");
    emitter.instruction("mov w11, #0x61");                                      // assume the lowercase base 'a'
    emitter.instruction("cmp w10, #0x61");                                      // below 'a'?
    emitter.instruction("b.lt __rt_asf_rot13_upper");                           // try the uppercase range instead
    emitter.instruction("cmp w10, #0x7A");                                      // within 'a'..'z'?
    emitter.instruction("b.le __rt_asf_rot13_apply");                           // a lowercase letter: rotate it
    emitter.label("__rt_asf_rot13_upper");
    emitter.instruction("mov w11, #0x41");                                      // switch to the uppercase base 'A'
    emitter.instruction("cmp w10, #0x41");                                      // below 'A'?
    emitter.instruction("b.lt __rt_asf_next");                                  // non-letter: leave unchanged
    emitter.instruction("cmp w10, #0x5A");                                      // above 'Z'?
    emitter.instruction("b.gt __rt_asf_next");                                  // non-letter: leave unchanged
    emitter.label("__rt_asf_rot13_apply");
    emitter.instruction("sub w10, w10, w11");                                   // letter index 0..25
    emitter.instruction("add w10, w10, #13");                                   // rotate by 13
    emitter.instruction("cmp w10, #26");                                        // past the end of the alphabet?
    emitter.instruction("b.lt __rt_asf_rot13_nowrap");                          // no wrap needed
    emitter.instruction("sub w10, w10, #26");                                   // wrap back into 0..25
    emitter.label("__rt_asf_rot13_nowrap");
    emitter.instruction("add w10, w10, w11");                                   // back to an ASCII letter

    emitter.label("__rt_asf_store");
    emitter.instruction("strb w10, [x1, x9]");                                  // write the transformed byte back
    emitter.label("__rt_asf_next");
    emitter.instruction("add x9, x9, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_asf_loop");                                     // continue the transform loop
    emitter.label("__rt_asf_done");
    // x2 already holds the input (and output) length for stateless transforms.
    emitter.instruction("ret");                                                 // return to the stream-filter caller

    // -- convert.base64-decode: walk 4-char groups, emit 3 bytes each.
    //    Non-base64 bytes (whitespace, '=' padding, others) are skipped.
    //    Output ≤ input, so in-place compaction is safe. --
    emitter.label("__rt_asf_b64_decode");
    emitter.instruction("mov x5, #0");                                          // read index
    emitter.instruction("mov x6, #0");                                          // write index
    emitter.instruction("mov x7, #0");                                          // 24-bit group accumulator
    emitter.instruction("mov x4, #0");                                          // chars in current group (0..3)
    emitter.label("__rt_asf_b64_loop");
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_b64_done");                              // finish the base64 decoder operation when the bound is reached
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    // Classify byte → 6-bit value or skip.
    emitter.instruction("cmp w8, #65");                                         // test for the lower bound of uppercase base64 letters
    emitter.instruction("b.lt __rt_asf_b64_try_digit");                         // reject values below the accepted range
    emitter.instruction("cmp w8, #90");                                         // test for the upper bound of uppercase base64 letters
    emitter.instruction("b.gt __rt_asf_b64_try_lower");                         // reject values above the accepted range
    emitter.instruction("sub w8, w8, #65");                                     // A..Z → 0..25
    emitter.instruction("b __rt_asf_b64_add");                                  // continue at __rt_asf_b64_add
    emitter.label("__rt_asf_b64_try_lower");
    emitter.instruction("cmp w8, #97");                                         // test for the lower bound of lowercase hex/base64 letters
    emitter.instruction("b.lt __rt_asf_b64_try_plus");                          // reject values below the accepted range
    emitter.instruction("cmp w8, #122");                                        // test for the upper bound of lowercase base64 letters
    emitter.instruction("b.gt __rt_asf_b64_try_plus");                          // reject values above the accepted range
    emitter.instruction("sub w8, w8, #71");                                     // a..z → 26..51 (97-26)
    emitter.instruction("b __rt_asf_b64_add");                                  // continue at __rt_asf_b64_add
    emitter.label("__rt_asf_b64_try_digit");
    emitter.instruction("cmp w8, #48");                                         // test for the lower bound of ASCII digits
    emitter.instruction("b.lt __rt_asf_b64_try_plus");                          // reject values below the accepted range
    emitter.instruction("cmp w8, #57");                                         // test for the upper bound of ASCII digits
    emitter.instruction("b.gt __rt_asf_b64_try_plus");                          // reject values above the accepted range
    emitter.instruction("add w8, w8, #4");                                      // 0..9 → 52..61
    emitter.instruction("b __rt_asf_b64_add");                                  // continue at __rt_asf_b64_add
    emitter.label("__rt_asf_b64_try_plus");
    emitter.instruction("cmp w8, #43");                                         // test for '+' in the base64 alphabet
    emitter.instruction("b.eq __rt_asf_b64_plus");                              // map '+' to its base64 sextet
    emitter.instruction("cmp w8, #47");                                         // test for '/' in the base64 alphabet
    emitter.instruction("b.eq __rt_asf_b64_slash");                             // map '/' to its base64 sextet
    emitter.instruction("b __rt_asf_b64_loop");                                 // skip everything else (ws, '=', etc.)
    emitter.label("__rt_asf_b64_plus");
    emitter.instruction("mov w8, #62");                                         // map '+' to base64 value 62
    emitter.instruction("b __rt_asf_b64_add");                                  // continue at __rt_asf_b64_add
    emitter.label("__rt_asf_b64_slash");
    emitter.instruction("mov w8, #63");                                         // map '/' to base64 value 63
    emitter.label("__rt_asf_b64_add");
    emitter.instruction("lsl x7, x7, #6");                                      // shift the accumulator to make room for the next value
    emitter.instruction("orr x7, x7, x8");                                      // merge the extracted bits into the accumulator
    emitter.instruction("add x4, x4, #1");                                      // advance the base64 group count
    emitter.instruction("cmp x4, #4");                                          // check whether a full four-character group is ready
    emitter.instruction("b.lt __rt_asf_b64_loop");                              // reject values below the accepted range
    // 24 bits accumulated → emit 3 bytes.
    emitter.instruction("ubfx w9, w7, #16, #8");                                // extract one decoded byte from the base64 accumulator
    emitter.instruction("strb w9, [x1, x6]");                                   // write output back into the stream buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("ubfx w9, w7, #8, #8");                                 // extract one decoded byte from the base64 accumulator
    emitter.instruction("strb w9, [x1, x6]");                                   // write output back into the stream buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("ubfx w9, w7, #0, #8");                                 // extract one decoded byte from the base64 accumulator
    emitter.instruction("strb w9, [x1, x6]");                                   // write output back into the stream buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("mov x4, #0");                                          // initialize the base64 group count
    emitter.instruction("mov x7, #0");                                          // initialize the filter accumulator or state flag
    emitter.instruction("b __rt_asf_b64_loop");                                 // continue the base64 decoder loop
    emitter.label("__rt_asf_b64_done");
    // Handle partial group (2 or 3 chars).
    emitter.instruction("cmp x4, #2");                                          // check whether a partial group can produce output
    emitter.instruction("b.lt __rt_asf_b64_finish");                            // reject values below the accepted range
    emitter.instruction("cmp x4, #3");                                          // check whether a full three-byte group is available
    emitter.instruction("b.eq __rt_asf_b64_three");                             // dispatch to __rt_asf_b64_three when the comparison matches
    // 2 chars: pad with 12 zero bits, emit 1 byte.
    emitter.instruction("lsl x7, x7, #12");                                     // shift the accumulator to make room for the next value
    emitter.instruction("ubfx w9, w7, #16, #8");                                // extract one decoded byte from the base64 accumulator
    emitter.instruction("strb w9, [x1, x6]");                                   // write output back into the stream buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("b __rt_asf_b64_finish");                               // continue at __rt_asf_b64_finish
    emitter.label("__rt_asf_b64_three");
    // 3 chars: pad with 6 zero bits, emit 2 bytes.
    emitter.instruction("lsl x7, x7, #6");                                      // shift the accumulator to make room for the next value
    emitter.instruction("ubfx w9, w7, #16, #8");                                // extract one decoded byte from the base64 accumulator
    emitter.instruction("strb w9, [x1, x6]");                                   // write output back into the stream buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("ubfx w9, w7, #8, #8");                                 // extract one decoded byte from the base64 accumulator
    emitter.instruction("strb w9, [x1, x6]");                                   // write output back into the stream buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.label("__rt_asf_b64_finish");
    emitter.instruction("mov x2, x6");                                          // return decoded length
    emitter.instruction("ret");                                                 // return to the stream-filter caller

    // -- dechunk: php-src's `php_dechunk` (ext/standard/filters.c:1724), transcribed.
    //
    //    The four rules elephc's own parser did not have, each MEASURED through php-src's
    //    `filters/chunked_001.phpt`, whose seven streams it answered two of:
    //      * The CR is OPTIONAL and the LF is what ends a line — an LF-terminated stream never
    //        ended a size line here at all, so `2\nte\n2\nst\n0\n` decoded to nothing.
    //      * A size line ends at the FIRST byte that is not a hex digit, and that byte opens the
    //        extension, skipped to a CR *or* an LF. Skipping non-hex bytes and accumulating past
    //        them read the `e` of `te` as another digit.
    //      * A size line whose first byte is not hex is php's CHUNK_ERROR.
    //      * So is a chunk body not followed by its LF. CHUNK_ERROR stops parsing and copies the
    //        remainder VERBATIM, which is how php answers a stream it cannot frame.
    //
    //    ⚠️ php carries this state ACROSS buckets; this helper does not — it restarts at
    //    CHUNK_SIZE_START on every call, so a chunk split over two reads still decodes wrong.
    //    That limit is older than this parser and is not addressed here.
    //
    //    x5 = read cursor, x6 = write cursor, x7 = chunk size, x10 = where the size line began. --
    emitter.label("__rt_asf_dechunk");
    emitter.instruction("mov x5, #0");                                          // read index
    emitter.instruction("mov x6, #0");                                          // write index
    emitter.label("__rt_asf_dechunk_size_start");
    emitter.instruction("cmp x5, x2");                                          // php's outer `while (p < end)`
    emitter.instruction("b.ge __rt_asf_dechunk_done");                          // nothing left to parse
    emitter.instruction("mov x7, #0");                                          // chunk size accumulator
    emitter.instruction("mov x10, x5");                                         // remember where the size line began
    emitter.label("__rt_asf_dechunk_size_scan");
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_done");                          // a size line cut short yields what was decoded
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("cmp w8, #48");                                         // test for the lower bound of ASCII digits
    emitter.instruction("b.lt __rt_asf_dechunk_size_nonhex");                   // not a hex digit: the size line is over
    emitter.instruction("cmp w8, #57");                                         // test for the upper bound of ASCII digits
    emitter.instruction("b.le __rt_asf_dechunk_size_digit");                    // 0..9
    emitter.instruction("orr w9, w8, #0x20");                                   // case-fold A..F onto a..f
    emitter.instruction("cmp w9, #97");                                         // test for the lower bound of lowercase hex letters
    emitter.instruction("b.lt __rt_asf_dechunk_size_nonhex");                   // not a hex digit: the size line is over
    emitter.instruction("cmp w9, #102");                                        // test for the upper bound of lowercase hex letters
    emitter.instruction("b.gt __rt_asf_dechunk_size_nonhex");                   // not a hex digit: the size line is over
    emitter.instruction("sub w9, w9, #87");                                     // a..f → 10..15 (97-87)
    emitter.instruction("b __rt_asf_dechunk_size_acc");                         // accumulate the digit
    emitter.label("__rt_asf_dechunk_size_digit");
    emitter.instruction("sub w9, w8, #48");                                     // 0..9 → 0..9
    emitter.label("__rt_asf_dechunk_size_acc");
    emitter.instruction("lsl x7, x7, #4");                                      // shift the accumulator to make room for the next value
    emitter.instruction("orr x7, x7, x9");                                      // merge the extracted bits into the accumulator
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("b __rt_asf_dechunk_size_scan");                        // continue at __rt_asf_dechunk_size_scan
    emitter.label("__rt_asf_dechunk_size_nonhex");
    emitter.instruction("cmp x5, x10");                                         // did the line hold ANY hex digit?
    emitter.instruction("b.eq __rt_asf_dechunk_error");                         // php's CHUNK_ERROR: a size line that starts non-hex
    emitter.label("__rt_asf_dechunk_ext");
    // CHUNK_SIZE_EXT: everything after the size is skipped, up to a CR or an LF.
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_done");                          // finish the dechunk operation when the bound is reached
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("cmp w8, #13");                                         // test for carriage return in the encoded stream
    emitter.instruction("b.eq __rt_asf_dechunk_size_cr");                       // the size line ends here
    emitter.instruction("cmp w8, #10");                                         // test for line feed in the encoded stream
    emitter.instruction("b.eq __rt_asf_dechunk_size_cr");                       // the size line ends here
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("b __rt_asf_dechunk_ext");                              // continue skipping the extension
    emitter.label("__rt_asf_dechunk_size_cr");
    // CHUNK_SIZE_CR: php consumes a CR only if one is there.
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("cmp w8, #13");                                         // test for carriage return in the encoded stream
    emitter.instruction("b.ne __rt_asf_dechunk_size_lf");                       // no CR: the LF is judged on this same byte
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_done");                          // finish the dechunk operation when the bound is reached
    emitter.label("__rt_asf_dechunk_size_lf");
    // CHUNK_SIZE_LF: the LF is REQUIRED — anything else is CHUNK_ERROR.
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("cmp w8, #10");                                         // test for line feed in the encoded stream
    emitter.instruction("b.ne __rt_asf_dechunk_error");                         // php's CHUNK_ERROR
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("cbz x7, __rt_asf_dechunk_done");                       // size 0 is the last chunk: php ignores the trailer
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_done");                          // finish the dechunk operation when the bound is reached
    // CHUNK_BODY: copy min(chunk size, what is left).
    emitter.instruction("sub x12, x2, x5");                                     // bytes still in the buffer
    emitter.instruction("cmp x12, x7");                                         // is the whole chunk here?
    emitter.instruction("b.lt __rt_asf_dechunk_body_copy");                     // a partial chunk copies what arrived
    emitter.instruction("mov x12, x7");                                         // the whole chunk
    emitter.label("__rt_asf_dechunk_body_copy");
    emitter.instruction("mov x11, #0");                                         // chunk-copy cursor
    emitter.label("__rt_asf_dechunk_copy_loop");
    emitter.instruction("cmp x11, x12");                                        // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_copy_done");                     // the chunk body is copied
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("strb w8, [x1, x6]");                                   // write output back into the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the chunk-copy cursor
    emitter.instruction("b __rt_asf_dechunk_copy_loop");                        // continue the dechunk loop
    emitter.label("__rt_asf_dechunk_copy_done");
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_done");                          // finish the dechunk operation when the bound is reached
    // CHUNK_BODY_CR / CHUNK_BODY_LF: the same optional CR, required LF pair.
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("cmp w8, #13");                                         // test for carriage return in the encoded stream
    emitter.instruction("b.ne __rt_asf_dechunk_body_lf");                       // no CR: the LF is judged on this same byte
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_done");                          // finish the dechunk operation when the bound is reached
    emitter.label("__rt_asf_dechunk_body_lf");
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("cmp w8, #10");                                         // test for line feed in the encoded stream
    emitter.instruction("b.ne __rt_asf_dechunk_error");                         // php's CHUNK_ERROR
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("b __rt_asf_dechunk_size_start");                       // the next chunk-size line
    emitter.label("__rt_asf_dechunk_error");
    // CHUNK_ERROR: php stops parsing and passes the remainder through untouched.
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_done");                          // finish the dechunk operation when the bound is reached
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("strb w8, [x1, x6]");                                   // write output back into the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("b __rt_asf_dechunk_error");                            // continue copying the remainder
    emitter.label("__rt_asf_dechunk_done");
    emitter.instruction("mov x2, x6");                                          // return the transformed output length
    emitter.instruction("ret");                                                 // return to the stream-filter caller

    // -- convert.quoted-printable-decode: parses '=XX' hex escapes and
    //    soft line breaks ('=\\r?\\n'). Non-escape bytes pass through.
    //    Output ≤ input; in-place compaction. Hex classification is
    //    inlined (no helper-call to keep x30 intact for the outer ret). --
    emitter.label("__rt_asf_qp_decode");
    emitter.instruction("mov x5, #0");                                          // read index
    emitter.instruction("mov x6, #0");                                          // write index
    emitter.label("__rt_asf_qp_loop");
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_qp_done");                               // finish the quoted-printable decoder operation when the bound is reached
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("cmp w8, #61");                                         // test for '=' before quoted-printable escaping
    emitter.instruction("b.eq __rt_asf_qp_escape");                             // escape the current byte
    emitter.instruction("strb w8, [x1, x6]");                                   // write output back into the stream buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("b __rt_asf_qp_loop");                                  // continue the quoted-printable decoder loop

    emitter.label("__rt_asf_qp_escape");
    // peek next byte; if \r or \n it's a soft line break.
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_qp_done");                               // finish the quoted-printable decoder operation when the bound is reached
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("cmp w8, #13");                                         // test for carriage return in the encoded stream
    emitter.instruction("b.eq __rt_asf_qp_soft_break");                         // handle a quoted-printable soft line break
    emitter.instruction("cmp w8, #10");                                         // test for line feed in the encoded stream
    emitter.instruction("b.eq __rt_asf_qp_soft_break_lf");                      // handle a quoted-printable soft line break
    // hex hi nibble (inlined classification, w9 = val or -1).
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("mov w9, #-1");                                         // mark the hex nibble as invalid until classification succeeds
    emitter.instruction("cmp w8, #48");                                         // test for the lower bound of ASCII digits
    emitter.instruction("b.lt __rt_asf_qp_hi_alpha");                           // reject values below the accepted range
    emitter.instruction("cmp w8, #57");                                         // test for the upper bound of ASCII digits
    emitter.instruction("b.gt __rt_asf_qp_hi_alpha");                           // reject values above the accepted range
    emitter.instruction("sub w9, w8, #48");                                     // convert an ASCII digit into its numeric value
    emitter.instruction("b __rt_asf_qp_hi_done");                               // continue at __rt_asf_qp_hi_done
    emitter.label("__rt_asf_qp_hi_alpha");
    emitter.instruction("orr w8, w8, #0x20");                                   // lowercase
    emitter.instruction("cmp w8, #97");                                         // test for the lower bound of lowercase hex/base64 letters
    emitter.instruction("b.lt __rt_asf_qp_hi_done");                            // reject values below the accepted range
    emitter.instruction("cmp w8, #102");                                        // test for line feed in the encoded stream
    emitter.instruction("b.gt __rt_asf_qp_hi_done");                            // reject values above the accepted range
    emitter.instruction("sub w9, w8, #87");                                     // convert a lowercase hex letter into its numeric value
    emitter.label("__rt_asf_qp_hi_done");
    emitter.instruction("cmp w9, #0");                                          // compare the current value for the next branch
    emitter.instruction("b.lt __rt_asf_qp_loop");                               // invalid hi → skip
    emitter.instruction("mov w10, w9");                                         // hi nibble saved
    // hex lo nibble.
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_qp_done");                               // finish the quoted-printable decoder operation when the bound is reached
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("mov w9, #-1");                                         // mark the hex nibble as invalid until classification succeeds
    emitter.instruction("cmp w8, #48");                                         // test for the lower bound of ASCII digits
    emitter.instruction("b.lt __rt_asf_qp_lo_alpha");                           // reject values below the accepted range
    emitter.instruction("cmp w8, #57");                                         // test for the upper bound of ASCII digits
    emitter.instruction("b.gt __rt_asf_qp_lo_alpha");                           // reject values above the accepted range
    emitter.instruction("sub w9, w8, #48");                                     // convert an ASCII digit into its numeric value
    emitter.instruction("b __rt_asf_qp_lo_done");                               // continue at __rt_asf_qp_lo_done
    emitter.label("__rt_asf_qp_lo_alpha");
    emitter.instruction("orr w8, w8, #0x20");                                   // merge the extracted bits into the accumulator
    emitter.instruction("cmp w8, #97");                                         // test for the lower bound of lowercase hex/base64 letters
    emitter.instruction("b.lt __rt_asf_qp_lo_done");                            // reject values below the accepted range
    emitter.instruction("cmp w8, #102");                                        // test for line feed in the encoded stream
    emitter.instruction("b.gt __rt_asf_qp_lo_done");                            // reject values above the accepted range
    emitter.instruction("sub w9, w8, #87");                                     // convert a lowercase hex letter into its numeric value
    emitter.label("__rt_asf_qp_lo_done");
    emitter.instruction("cmp w9, #0");                                          // compare the current value for the next branch
    emitter.instruction("b.lt __rt_asf_qp_loop");                               // invalid lo → skip
    emitter.instruction("lsl w10, w10, #4");                                    // move the high nibble into byte position
    emitter.instruction("orr w10, w10, w9");                                    // merge the extracted bits into the accumulator
    emitter.instruction("strb w10, [x1, x6]");                                  // write output back into the stream buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("b __rt_asf_qp_loop");                                  // continue the quoted-printable decoder loop

    emitter.label("__rt_asf_qp_soft_break");
    emitter.instruction("add x5, x5, #1");                                      // skip \r
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_qp_loop");                               // leave the loop when the cursor reaches its bound
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("cmp w8, #10");                                         // test for line feed in the encoded stream
    emitter.instruction("b.ne __rt_asf_qp_loop");                               // continue the quoted-printable decoder loop when the delimiter was not found
    emitter.instruction("add x5, x5, #1");                                      // and \n if present
    emitter.instruction("b __rt_asf_qp_loop");                                  // continue the quoted-printable decoder loop
    emitter.label("__rt_asf_qp_soft_break_lf");
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("b __rt_asf_qp_loop");                                  // continue the quoted-printable decoder loop

    emitter.label("__rt_asf_qp_done");
    emitter.instruction("mov x2, x6");                                          // return the transformed output length
    emitter.instruction("ret");                                                 // return to the stream-filter caller

    // -- convert.base64-encode: encode 3-byte groups to 4 base64 chars + '='
    //    padding. Output is 4/3 of input; we encode into _stream_grow_scratch
    //    and memcpy back. Caps input at 49152 bytes to keep the 65536-byte
    //    output inside the scratch. --
    emitter.label("__rt_asf_b64_encode");
    // Cap input length so the encoded output fits the 64KB scratch.
    emitter.instruction("mov x4, #49152");                                      // 49152 = 64KB * 3/4
    emitter.instruction("cmp x2, x4");                                          // check whether the current cursor reached its bound
    emitter.instruction("csel x2, x4, x2, gt");                                 // x2 = MIN(x2, 49152)
    crate::codegen_support::abi::emit_symbol_address(emitter, "x4", "_stream_grow_scratch");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x15", "_b64_encode_tbl");
    emitter.instruction("mov x5, #0");                                          // read index
    emitter.instruction("mov x6, #0");                                          // write index
    emitter.label("__rt_asf_b64e_loop");
    emitter.instruction("sub x7, x2, x5");                                      // bytes remaining
    emitter.instruction("cmp x7, #3");                                          // check whether a full three-byte group is available
    emitter.instruction("b.lt __rt_asf_b64e_rem");                              // reject values below the accepted range
    // Read 3 bytes.
    emitter.instruction("ldrb w8, [x1, x5]");                                   // byte 0
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("ldrb w9, [x1, x5]");                                   // byte 1
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("ldrb w10, [x1, x5]");                                  // byte 2
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    // char 0: byte0 >> 2
    emitter.instruction("lsr w11, w8, #2");                                     // extract the next sextet or byte from the accumulated bits
    emitter.instruction("ldrb w11, [x15, x11]");                                // load the base64 alphabet byte for the sextet
    emitter.instruction("strb w11, [x4, x6]");                                  // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    // char 1: ((byte0 & 3) << 4) | (byte1 >> 4)
    emitter.instruction("and w11, w8, #3");                                     // mask the bits needed for the next encoded byte
    emitter.instruction("lsl w11, w11, #4");                                    // shift bits into the position required by the output byte
    emitter.instruction("lsr w12, w9, #4");                                     // extract the next sextet or byte from the accumulated bits
    emitter.instruction("orr w11, w11, w12");                                   // merge the extracted bits into the accumulator
    emitter.instruction("ldrb w11, [x15, x11]");                                // load the base64 alphabet byte for the sextet
    emitter.instruction("strb w11, [x4, x6]");                                  // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    // char 2: ((byte1 & 15) << 2) | (byte2 >> 6)
    emitter.instruction("and w11, w9, #15");                                    // mask the bits needed for the next encoded byte
    emitter.instruction("lsl w11, w11, #2");                                    // shift bits into the position required by the output byte
    emitter.instruction("lsr w12, w10, #6");                                    // extract the next sextet or byte from the accumulated bits
    emitter.instruction("orr w11, w11, w12");                                   // merge the extracted bits into the accumulator
    emitter.instruction("ldrb w11, [x15, x11]");                                // load the base64 alphabet byte for the sextet
    emitter.instruction("strb w11, [x4, x6]");                                  // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    // char 3: byte2 & 0x3f
    emitter.instruction("and w11, w10, #0x3f");                                 // mask the bits needed for the next encoded byte
    emitter.instruction("ldrb w11, [x15, x11]");                                // load the base64 alphabet byte for the sextet
    emitter.instruction("strb w11, [x4, x6]");                                  // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("b __rt_asf_b64e_loop");                                // continue the base64 encoder loop
    emitter.label("__rt_asf_b64e_rem");
    emitter.instruction("cbz x7, __rt_asf_b64e_copyback");                      // finish the base64 encoder operation when the count is zero
    emitter.instruction("cmp x7, #1");                                          // check whether only one byte remains
    emitter.instruction("b.eq __rt_asf_b64e_rem1");                             // handle a one-byte base64 tail
    // 2-byte remainder: 3 chars + 1 padding.
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("ldrb w9, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("lsr w11, w8, #2");                                     // extract the next sextet or byte from the accumulated bits
    emitter.instruction("ldrb w11, [x15, x11]");                                // load the base64 alphabet byte for the sextet
    emitter.instruction("strb w11, [x4, x6]");                                  // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("and w11, w8, #3");                                     // mask the bits needed for the next encoded byte
    emitter.instruction("lsl w11, w11, #4");                                    // shift bits into the position required by the output byte
    emitter.instruction("lsr w12, w9, #4");                                     // extract the next sextet or byte from the accumulated bits
    emitter.instruction("orr w11, w11, w12");                                   // merge the extracted bits into the accumulator
    emitter.instruction("ldrb w11, [x15, x11]");                                // load the base64 alphabet byte for the sextet
    emitter.instruction("strb w11, [x4, x6]");                                  // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("and w11, w9, #15");                                    // mask the bits needed for the next encoded byte
    emitter.instruction("lsl w11, w11, #2");                                    // shift bits into the position required by the output byte
    emitter.instruction("ldrb w11, [x15, x11]");                                // load the base64 alphabet byte for the sextet
    emitter.instruction("strb w11, [x4, x6]");                                  // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("mov w11, #61");                                        // '='
    emitter.instruction("strb w11, [x4, x6]");                                  // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("b __rt_asf_b64e_copyback");                            // copy the scratch output back into the stream buffer
    emitter.label("__rt_asf_b64e_rem1");
    // 1-byte remainder: 2 chars + 2 padding.
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("lsr w11, w8, #2");                                     // extract the next sextet or byte from the accumulated bits
    emitter.instruction("ldrb w11, [x15, x11]");                                // load the base64 alphabet byte for the sextet
    emitter.instruction("strb w11, [x4, x6]");                                  // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("and w11, w8, #3");                                     // mask the bits needed for the next encoded byte
    emitter.instruction("lsl w11, w11, #4");                                    // shift bits into the position required by the output byte
    emitter.instruction("ldrb w11, [x15, x11]");                                // load the base64 alphabet byte for the sextet
    emitter.instruction("strb w11, [x4, x6]");                                  // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("mov w11, #61");                                        // '='
    emitter.instruction("strb w11, [x4, x6]");                                  // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("strb w11, [x4, x6]");                                  // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.label("__rt_asf_b64e_copyback");
    emit_wrapped_copyback_aarch64(emitter, "b64e");
    emitter.instruction("ret");                                                 // return to the stream-filter caller

    // -- convert.quoted-printable-encode: php's `php_conv_qprint_encode_convert`, including
    //    its line-break passthrough and trailing-whitespace rule. Encodes into
    //    _stream_grow_scratch and copies back. Caps input at 21845 bytes so the worst case
    //    fits 65536, and stops writing at the scratch bound rather than past it. --
    emitter.label("__rt_asf_qp_encode");
    emitter.instruction("mov x4, #21845");                                                              // ~64KB/3 worst-case cap
    emitter.instruction("cmp x2, x4");                                                                  // clamp the input to what the scratch can hold
    emitter.instruction("csel x2, x4, x2, gt");                                                         // x2 = MIN(x2, 21845)
    crate::codegen_support::abi::emit_symbol_address(emitter, "x4", "_stream_grow_scratch");

    // -- the three parameters, read ONCE: they cannot change while one buffer is encoded --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_asf_binary");
    emitter.instruction("ldr x13, [x9]");                                                               // do the binary rules apply?
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_asf_line_length");
    emitter.instruction("ldr x15, [x9]");                                                               // the width php wraps at
    // php's factory: `line-length < 4` nulls the break chars, which turns OFF wrapping, the
    // line-break passthrough and the trailing-whitespace rule together. Below that width a
    // soft break plus `=XX` could not fit on one line at all.
    emitter.instruction("cmp x15, #4");
    emitter.instruction("b.ge __rt_asf_qpe_lb_on");                                                     // wide enough: the break chars are in force
    emitter.instruction("mov x15, #0");                                                                 // no wrapping
    emitter.instruction("mov x0, #0");                                                                  // and no break chars
    emitter.instruction("mov x3, #0");
    emitter.instruction("b __rt_asf_qpe_lb_ready");
    emitter.label("__rt_asf_qpe_lb_on");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_asf_break_ptr");
    emitter.instruction("ldr x0, [x9]");                                                                // the caller's break bytes
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_asf_break_len");
    emitter.instruction("ldr x3, [x9]");
    emitter.instruction("cbnz x0, __rt_asf_qpe_lb_ready");                                              // the caller named its own
    crate::codegen_support::abi::emit_symbol_address(emitter, "x0", "_filter_break_crlf");
    emitter.instruction("mov x3, #2");                                                                  // php's default break is CRLF
    emitter.label("__rt_asf_qpe_lb_ready");
    emitter.instruction("mov x5, #0");                                                                  // read index
    emitter.instruction("mov x6, #0");                                                                  // write index
    emitter.instruction("mov x14, #0");                                                                 // columns used on the current line
    emitter.instruction("mov x7, #0");                                                                  // lb_cnt: how much of the break sequence has matched
    emitter.instruction("mov x16, #0");                                                                 // lb_ptr: how much of a broken-off match has been replayed
    emitter.instruction("mov x17, #0");                                                                 // trail_ws: how many bytes of a line-ending run are still to escape

    emitter.label("__rt_asf_qpe_loop");
    emitter.instruction("mov x9, #65500");
    emitter.instruction("cmp x6, x9");
    emitter.instruction("b.ge __rt_asf_qpe_copyback");                                                  // never write past the scratch, whatever the input
    // -- php's lookahead: a byte continuing the break sequence is consumed HERE and never
    //    reaches the encoder --
    emitter.instruction("cbnz x13, __rt_asf_qpe_no_lb");                                                // binary: the sequence is not special
    emitter.instruction("cbz x0, __rt_asf_qpe_no_lb");                                                  // no break chars in force
    emitter.instruction("cbz x3, __rt_asf_qpe_no_lb");                                                  // a zero-length break matches nothing
    emitter.instruction("cmp x5, x2");
    emitter.instruction("b.ge __rt_asf_qpe_no_lb");                                                     // nothing left to look at
    emitter.instruction("ldrb w9, [x1, x5]");                                                           // the byte under the cursor
    emitter.instruction("ldrb w10, [x0, x7]");                                                          // the break byte it has to match
    emitter.instruction("cmp w9, w10");
    emitter.instruction("b.ne __rt_asf_qpe_no_lb");
    emitter.instruction("add x7, x7, #1");                                                              // one more byte of the sequence matched
    emitter.instruction("add x5, x5, #1");
    emitter.instruction("cmp x7, x3");
    emitter.instruction("b.lt __rt_asf_qpe_loop");                                                      // a partial match: keep looking
    // The whole sequence: php copies it out VERBATIM and starts a fresh line.
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_asf_qpe_lb_emit");
    emitter.instruction("cmp x11, x7");
    emitter.instruction("b.ge __rt_asf_qpe_lb_emitted");
    emitter.instruction("ldrb w12, [x0, x11]");
    emitter.instruction("strb w12, [x4, x6]");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_asf_qpe_lb_emit");
    emitter.label("__rt_asf_qpe_lb_emitted");
    emitter.instruction("mov x14, #0");                                                                 // the break restarts the line budget
    emitter.instruction("mov x7, #0");
    emitter.instruction("mov x16, #0");
    emitter.instruction("b __rt_asf_qpe_loop");

    emitter.label("__rt_asf_qpe_no_lb");
    // php: `if (lb_ptr >= lb_cnt && icnt == 0) break;` — a broken-off partial match still
    // has bytes to replay even when the input is exhausted.
    emitter.instruction("cmp x16, x7");
    emitter.instruction("b.lt __rt_asf_qpe_have");
    emitter.instruction("cmp x5, x2");
    emitter.instruction("b.ge __rt_asf_qpe_copyback");
    emitter.label("__rt_asf_qpe_have");
    // c = lb_ptr < lb_cnt ? lbchars[lb_ptr] : *ps
    emitter.instruction("cmp x16, x7");
    emitter.instruction("b.ge __rt_asf_qpe_from_input");
    emitter.instruction("ldrb w8, [x0, x16]");                                                          // replay a byte of the match that broke off
    emitter.instruction("b __rt_asf_qpe_classify");
    emitter.label("__rt_asf_qpe_from_input");
    emitter.instruction("ldrb w8, [x1, x5]");
    emitter.label("__rt_asf_qpe_classify");
    emitter.instruction("cbnz x13, __rt_asf_qpe_range");                                                // binary escapes SPACE and TAB like any other byte
    emitter.instruction("cbnz x17, __rt_asf_qpe_range");                                                // a line-ending run already decided: escape
    emitter.instruction("cmp w8, #9");
    emitter.instruction("b.eq __rt_asf_qpe_ws");
    emitter.instruction("cmp w8, #32");
    emitter.instruction("b.eq __rt_asf_qpe_ws");
    emitter.label("__rt_asf_qpe_range");
    emitter.instruction("cmp w8, #33");
    emitter.instruction("b.lt __rt_asf_qpe_escape");
    emitter.instruction("cmp w8, #126");
    emitter.instruction("b.gt __rt_asf_qpe_escape");
    emitter.instruction("cmp w8, #61");                                                                 // '=' is escaped even though it is printable
    emitter.instruction("b.eq __rt_asf_qpe_escape");

    // -- a printable byte: one column --
    emitter.label("__rt_asf_qpe_literal");
    emitter.instruction("cbz x0, __rt_asf_qpe_lit_fits");                                               // no break chars means no wrapping
    emitter.instruction("add x9, x14, #1");
    emitter.instruction("sub x11, x15, #1");                                                            // the `=` needs the last column
    emitter.instruction("cmp x9, x11");
    emitter.instruction("b.le __rt_asf_qpe_lit_fits");
    emitter.instruction("mov w9, #61");
    emitter.instruction("strb w9, [x4, x6]");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_asf_qpe_lit_bloop");
    emitter.instruction("cmp x11, x3");
    emitter.instruction("b.ge __rt_asf_qpe_lit_bend");
    emitter.instruction("ldrb w12, [x0, x11]");
    emitter.instruction("strb w12, [x4, x6]");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_asf_qpe_lit_bloop");
    emitter.label("__rt_asf_qpe_lit_bend");
    emitter.instruction("mov x14, #0");
    emitter.label("__rt_asf_qpe_lit_fits");
    emitter.instruction("strb w8, [x4, x6]");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("add x14, x14, #1");
    emitter.instruction("b __rt_asf_qpe_consume1");

    // -- SPACE or TAB. php writes it literally UNLESS it is the run that ends a line: the
    //    same byte is literal mid-line and `=20` before a break, so it has to look ahead --
    emitter.label("__rt_asf_qpe_ws");
    emitter.instruction("cbz x0, __rt_asf_qpe_ws_literal");                                             // php only looks ahead when it knows the break
    emitter.instruction("add x9, x14, #1");
    emitter.instruction("sub x11, x15, #1");
    emitter.instruction("cmp x9, x11");
    emitter.instruction("b.le __rt_asf_qpe_ws_scan");
    // The budget is gone: php wraps FIRST, without consuming the byte, and re-decides.
    emitter.instruction("mov w9, #61");
    emitter.instruction("strb w9, [x4, x6]");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_asf_qpe_ws_bloop");
    emitter.instruction("cmp x11, x3");
    emitter.instruction("b.ge __rt_asf_qpe_ws_bend");
    emitter.instruction("ldrb w12, [x0, x11]");
    emitter.instruction("strb w12, [x4, x6]");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_asf_qpe_ws_bloop");
    emitter.label("__rt_asf_qpe_ws_bend");
    emitter.instruction("mov x14, #0");
    emitter.instruction("b __rt_asf_qpe_loop");                                                         // re-decide on a fresh line
    emitter.label("__rt_asf_qpe_ws_scan");
    // php scans from THIS byte to the last-but-one of the buffer: a whitespace run that
    // reaches the break chars (or the buffer end) is trailing, anything else is not.
    emitter.instruction("mov x17, #1");                                                                 // trail_ws counts this byte too
    emitter.instruction("mov x10, #0");                                                                 // lb_cnt2
    emitter.instruction("mov x12, x5");                                                                 // ps2
    emitter.label("__rt_asf_qpe_ws_sloop");
    emitter.instruction("sub x9, x2, #1");
    emitter.instruction("cmp x12, x9");
    emitter.instruction("b.ge __rt_asf_qpe_ws_sdone");                                                  // ran out: the run ends the buffer, so it is trailing
    emitter.instruction("ldrb w11, [x1, x12]");
    emitter.instruction("ldrb w9, [x0, x10]");
    emitter.instruction("cmp w11, w9");
    emitter.instruction("b.ne __rt_asf_qpe_ws_nomatch");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("cmp x10, x3");
    emitter.instruction("b.ge __rt_asf_qpe_ws_sdone");                                                  // the run does end a line
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_asf_qpe_ws_sloop");
    emitter.label("__rt_asf_qpe_ws_nomatch");
    emitter.instruction("cbnz x10, __rt_asf_qpe_ws_notrail");                                           // a partial break match broke off
    emitter.instruction("cmp w11, #9");
    emitter.instruction("b.eq __rt_asf_qpe_ws_more");
    emitter.instruction("cmp w11, #32");
    emitter.instruction("b.eq __rt_asf_qpe_ws_more");
    emitter.label("__rt_asf_qpe_ws_notrail");
    emitter.instruction("mov x17, #0");                                                                 // real content follows: nothing here is trailing
    emitter.instruction("b __rt_asf_qpe_ws_sdone");
    emitter.label("__rt_asf_qpe_ws_more");
    emitter.instruction("add x17, x17, #1");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_asf_qpe_ws_sloop");
    emitter.label("__rt_asf_qpe_ws_sdone");
    emitter.instruction("cbnz x17, __rt_asf_qpe_loop");                                                 // trailing: go round and take the escape path
    emitter.label("__rt_asf_qpe_ws_literal");
    emitter.instruction("strb w8, [x4, x6]");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("add x14, x14, #1");
    emitter.instruction("b __rt_asf_qpe_consume1");

    // -- anything else becomes `=XX`: three columns --
    emitter.label("__rt_asf_qpe_escape");
    emitter.instruction("cbz x0, __rt_asf_qpe_esc_fits");
    emitter.instruction("add x9, x14, #3");
    emitter.instruction("sub x11, x15, #1");
    emitter.instruction("cmp x9, x11");
    emitter.instruction("b.le __rt_asf_qpe_esc_fits");
    emitter.instruction("mov w9, #61");
    emitter.instruction("strb w9, [x4, x6]");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("mov x11, #0");
    emitter.label("__rt_asf_qpe_esc_bloop");
    emitter.instruction("cmp x11, x3");
    emitter.instruction("b.ge __rt_asf_qpe_esc_bend");
    emitter.instruction("ldrb w12, [x0, x11]");
    emitter.instruction("strb w12, [x4, x6]");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_asf_qpe_esc_bloop");
    emitter.label("__rt_asf_qpe_esc_bend");
    emitter.instruction("mov x14, #0");
    emitter.label("__rt_asf_qpe_esc_fits");
    emitter.instruction("add x14, x14, #3");                                                            // `=XX` occupies three columns
    emitter.instruction("mov w9, #61");
    emitter.instruction("strb w9, [x4, x6]");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("lsr w9, w8, #4");
    emitter.instruction("and w9, w9, #0xF");
    emitter.instruction("cmp w9, #10");
    emitter.instruction("b.lt __rt_asf_qpe_hi_dig");
    emitter.instruction("add w9, w9, #55");                                                             // 10 -> 'A'
    emitter.instruction("b __rt_asf_qpe_hi_write");
    emitter.label("__rt_asf_qpe_hi_dig");
    emitter.instruction("add w9, w9, #48");                                                             // 0 -> '0'
    emitter.label("__rt_asf_qpe_hi_write");
    emitter.instruction("strb w9, [x4, x6]");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("and w9, w8, #0xF");
    emitter.instruction("cmp w9, #10");
    emitter.instruction("b.lt __rt_asf_qpe_lo_dig");
    emitter.instruction("add w9, w9, #55");
    emitter.instruction("b __rt_asf_qpe_lo_write");
    emitter.label("__rt_asf_qpe_lo_dig");
    emitter.instruction("add w9, w9, #48");
    emitter.label("__rt_asf_qpe_lo_write");
    emitter.instruction("strb w9, [x4, x6]");
    emitter.instruction("add x6, x6, #1");
    // php charges one byte of the line-ending run per escape it emits.
    emitter.instruction("cbz x17, __rt_asf_qpe_consume1");
    emitter.instruction("sub x17, x17, #1");

    // -- CONSUME_CHAR: a replayed byte advances the replay cursor, a real one the input --
    emitter.label("__rt_asf_qpe_consume1");
    emitter.instruction("cmp x16, x7");
    emitter.instruction("b.ge __rt_asf_qpe_consume_input");
    emitter.instruction("add x16, x16, #1");
    emitter.instruction("b __rt_asf_qpe_loop");
    emitter.label("__rt_asf_qpe_consume_input");
    emitter.instruction("mov x7, #0");
    emitter.instruction("mov x16, #0");
    emitter.instruction("add x5, x5, #1");
    emitter.instruction("b __rt_asf_qpe_loop");

    emitter.label("__rt_asf_qpe_copyback");
    // A plain copy: the soft breaks are already in the scratch. Wrapping here as base64 does would
    // break the SAME output twice, and php's quoted-printable break is not a plain separator — it
    // is a `=` that occupies a column and must not fall inside an `=XX` triplet, which only the
    // encoder knows.
    emit_plain_copyback_aarch64(emitter, "qpe");
    emitter.instruction("ret");                                                 // return to the stream-filter caller
}

/// Copies `scratch[0..x6]` into the buffer at `x1`, breaking lines at the published `line-length`.
///
/// On entry `x4` is the scratch base and `x6` the encoded length; on exit `x2` is the length
/// actually WRITTEN, which exceeds `x6` whenever a break was inserted. `x1` is the caller's own
/// buffer: every caller of `__rt_apply_stream_filter` hands it one sized for the worst-case growth
/// its filter can produce, and both encoders cap their input to stay inside it.
///
/// php's `convert.base64-encode` and `convert.quoted-printable-encode` take `line-length` and
/// `line-break-chars` from `$params` and wrap at NOTHING when neither is given — which is why a
/// zero length takes the plain-copy path, so the common case pays one compare. Measured on
/// `php -n` 8.5.6: `["line-length" => 8]` over "hello world, this is a test" answers five
/// 8-character lines separated by `\n` with NO trailing break, and `line-break-chars` replaces the
/// `\n` verbatim.
fn emit_wrapped_copyback_aarch64(emitter: &mut Emitter, tag: &str) {
    let plain = format!("__rt_asf_{tag}_cb_plain");
    let plain_loop = format!("__rt_asf_{tag}_cb_ploop");
    let wrap_loop = format!("__rt_asf_{tag}_cb_wloop");
    let no_break = format!("__rt_asf_{tag}_cb_nobreak");
    let have_break = format!("__rt_asf_{tag}_cb_have");
    let brk_head = format!("__rt_asf_{tag}_cb_brk");
    let brk_end = format!("__rt_asf_{tag}_cb_brkend");
    let finish = format!("__rt_asf_{tag}_cb_fin");

    crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_asf_line_length");
    emitter.instruction("ldr x13, [x13]");                                      // the width the caller asked for
    emitter.instruction(&format!("cbz x13, {plain}"));                          // php wraps at nothing by default

    // -- wrapping copy: x5 reads the scratch, x7 writes the buffer, x14 counts the line --
    emitter.instruction("mov x5, #0");                                          // scratch read cursor
    emitter.instruction("mov x7, #0");                                          // buffer write cursor
    emitter.instruction("mov x14, #0");                                         // characters on the current line
    emitter.label(&wrap_loop);
    emitter.instruction("cmp x5, x6");                                          // copied every encoded byte?
    emitter.instruction(&format!("b.ge {finish}"));
    emitter.instruction("cmp x14, x13");                                        // is the current line full?
    emitter.instruction(&format!("b.lt {no_break}"));                           // no: keep filling it
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_asf_break_ptr");
    emitter.instruction("ldr x9, [x9]");                                        // the caller's break bytes
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_asf_break_len");
    emitter.instruction("ldr x10, [x10]");
    emitter.instruction(&format!("cbnz x9, {have_break}"));
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_filter_break_crlf");
    emitter.instruction("mov x10, #2");                                         // php's default break is CRLF
    emitter.label(&have_break);
    emitter.instruction("mov x11, #0");                                         // break-byte cursor
    emitter.label(&brk_head);
    emitter.instruction("cmp x11, x10");                                        // written the whole break?
    emitter.instruction(&format!("b.ge {brk_end}"));
    emitter.instruction("ldrb w12, [x9, x11]");                                 // one break byte
    emitter.instruction("strb w12, [x1, x7]");
    emitter.instruction("add x7, x7, #1");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction(&format!("b {brk_head}"));
    emitter.label(&brk_end);
    emitter.instruction("mov x14, #0");                                         // the new line starts empty
    emitter.label(&no_break);
    emitter.instruction("ldrb w12, [x4, x5]");                                  // one encoded byte
    emitter.instruction("strb w12, [x1, x7]");
    emitter.instruction("add x5, x5, #1");
    emitter.instruction("add x7, x7, #1");
    emitter.instruction("add x14, x14, #1");
    emitter.instruction(&format!("b {wrap_loop}"));

    // -- plain copy: the unparameterized path, byte for byte --
    emitter.label(&plain);
    emitter.instruction("mov x5, #0");                                          // initialize the read cursor
    emitter.instruction("mov x7, x6");                                          // the written length equals the encoded one
    emitter.label(&plain_loop);
    emitter.instruction("cmp x5, x6");                                          // check whether the current cursor reached its bound
    emitter.instruction(&format!("b.ge {finish}"));                             // finish when the bound is reached
    emitter.instruction("ldrb w11, [x4, x5]");                                  // load the next byte from the scratch buffer
    emitter.instruction("strb w11, [x1, x5]");                                  // copy scratch output back into the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction(&format!("b {plain_loop}"));

    emitter.label(&finish);
    emitter.instruction("mov x2, x7");                                          // return what was WRITTEN, breaks included
}

/// Copies `scratch[0..x6]` into the buffer at `x1` byte for byte, and answers the length in `x2`.
fn emit_plain_copyback_aarch64(emitter: &mut Emitter, tag: &str) {
    let loop_label = format!("__rt_asf_{tag}_cb_loop");
    let done = format!("__rt_asf_{tag}_cb_done");
    emitter.instruction("mov x5, #0");                                          // initialize the read cursor
    emitter.label(&loop_label);
    emitter.instruction("cmp x5, x6");                                          // check whether the current cursor reached its bound
    emitter.instruction(&format!("b.ge {done}"));                               // finish when the bound is reached
    emitter.instruction("ldrb w11, [x4, x5]");                                  // load the next byte from the scratch buffer
    emitter.instruction("strb w11, [x1, x5]");                                  // copy scratch output back into the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction(&format!("b {loop_label}"));
    emitter.label(&done);
    emitter.instruction("mov x2, x6");                                          // return the encoded length
}


/// Emits the Linux x86_64 stream runtime helper for apply stream filter.
fn emit_apply_stream_filter_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: apply_stream_filter ---");
    emitter.label_global("__rt_apply_stream_filter");

    emitter.instruction("xor r9, r9");                                          // r9 = current byte index
    emitter.label("__rt_asf_loop_x86");
    emitter.instruction("cmp r9, rdx");                                         // processed every byte?
    emitter.instruction("jge __rt_asf_done_x86");                               // stop when the buffer is exhausted
    emitter.instruction("movzx r10d, BYTE PTR [rax + r9]");                     // load the current byte
    emitter.instruction("cmp rcx, 1");                                          // filter id 1 = string.toupper
    emitter.instruction("je __rt_asf_upper_x86");                               // dispatch to the uppercase transform
    emitter.instruction("cmp rcx, 2");                                          // filter id 2 = string.tolower
    emitter.instruction("je __rt_asf_lower_x86");                               // dispatch to the lowercase transform
    emitter.instruction("cmp rcx, 3");                                          // filter id 3 = string.rot13
    emitter.instruction("je __rt_asf_rot13_x86");                               // dispatch to the rot13 transform
    emitter.instruction("cmp rcx, 7");                                          // filter id 7 = convert.base64-decode
    emitter.instruction("je __rt_asf_b64_decode_x86");                          // dispatch to the base64-decode state machine
    emitter.instruction("cmp rcx, 5");                                          // filter id 5 = dechunk
    emitter.instruction("je __rt_asf_dechunk_x86");                             // dispatch to the HTTP/1.1 chunked-encoding parser
    emitter.instruction("cmp rcx, 6");                                          // filter id 6 = convert.base64-encode
    emitter.instruction("je __rt_asf_b64_encode_x86");                          // dispatch to the base64-encode helper
    emitter.instruction("cmp rcx, 9");                                          // filter id 9 = convert.quoted-printable-decode
    emitter.instruction("je __rt_asf_qp_decode_x86");                           // dispatch to the QP decoder
    emitter.instruction("cmp rcx, 8");                                          // filter id 8 = convert.quoted-printable-encode
    emitter.instruction("je __rt_asf_qp_encode_x86");                           // dispatch to the QP encoder
    emitter.instruction("jmp __rt_asf_next_x86");                               // unknown id: leave the byte unchanged

    emitter.label("__rt_asf_upper_x86");
    emitter.instruction("cmp r10b, 0x61");                                      // below 'a'?
    emitter.instruction("jl __rt_asf_next_x86");                                // non-letter: leave unchanged
    emitter.instruction("cmp r10b, 0x7A");                                      // above 'z'?
    emitter.instruction("jg __rt_asf_next_x86");                                // non-letter: leave unchanged
    emitter.instruction("sub r10b, 0x20");                                      // lowercase -> uppercase
    emitter.instruction("jmp __rt_asf_store_x86");                              // store the transformed byte

    emitter.label("__rt_asf_lower_x86");
    emitter.instruction("cmp r10b, 0x41");                                      // below 'A'?
    emitter.instruction("jl __rt_asf_next_x86");                                // non-letter: leave unchanged
    emitter.instruction("cmp r10b, 0x5A");                                      // above 'Z'?
    emitter.instruction("jg __rt_asf_next_x86");                                // non-letter: leave unchanged
    emitter.instruction("add r10b, 0x20");                                      // uppercase -> lowercase
    emitter.instruction("jmp __rt_asf_store_x86");                              // store the transformed byte

    emitter.label("__rt_asf_rot13_x86");
    emitter.instruction("mov r11b, 0x61");                                      // assume the lowercase base 'a'
    emitter.instruction("cmp r10b, 0x61");                                      // below 'a'?
    emitter.instruction("jl __rt_asf_rot13_upper_x86");                         // try the uppercase range instead
    emitter.instruction("cmp r10b, 0x7A");                                      // within 'a'..'z'?
    emitter.instruction("jle __rt_asf_rot13_apply_x86");                        // a lowercase letter: rotate it
    emitter.label("__rt_asf_rot13_upper_x86");
    emitter.instruction("mov r11b, 0x41");                                      // switch to the uppercase base 'A'
    emitter.instruction("cmp r10b, 0x41");                                      // below 'A'?
    emitter.instruction("jl __rt_asf_next_x86");                                // non-letter: leave unchanged
    emitter.instruction("cmp r10b, 0x5A");                                      // above 'Z'?
    emitter.instruction("jg __rt_asf_next_x86");                                // non-letter: leave unchanged
    emitter.label("__rt_asf_rot13_apply_x86");
    emitter.instruction("sub r10b, r11b");                                      // letter index 0..25
    emitter.instruction("add r10b, 13");                                        // rotate by 13
    emitter.instruction("cmp r10b, 26");                                        // past the end of the alphabet?
    emitter.instruction("jl __rt_asf_rot13_nowrap_x86");                        // no wrap needed
    emitter.instruction("sub r10b, 26");                                        // wrap back into 0..25
    emitter.label("__rt_asf_rot13_nowrap_x86");
    emitter.instruction("add r10b, r11b");                                      // back to an ASCII letter

    emitter.label("__rt_asf_store_x86");
    emitter.instruction("mov BYTE PTR [rax + r9], r10b");                       // write the transformed byte back
    emitter.label("__rt_asf_next_x86");
    emitter.instruction("inc r9");                                              // advance to the next byte
    emitter.instruction("jmp __rt_asf_loop_x86");                               // continue the transform loop
    emitter.label("__rt_asf_done_x86");
    // rdx already holds the input (and output) length for stateless transforms.
    emitter.instruction("ret");                                                 // return to the stream-filter caller

    // -- convert.base64-decode (x86_64) --
    emitter.label("__rt_asf_b64_decode_x86");
    emitter.instruction("xor r9, r9");                                          // read index
    emitter.instruction("xor r10, r10");                                        // write index
    emitter.instruction("xor r11, r11");                                        // 24-bit accumulator
    emitter.instruction("xor r12, r12");                                        // chars in group
    emitter.label("__rt_asf_b64_loop_x86");
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_b64_done_x86");                           // finish the base64 decoder operation when the bound is reached
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("cmp r8b, 65");                                         // test for the lower bound of uppercase base64 letters
    emitter.instruction("jl __rt_asf_b64_try_digit_x86");                       // reject values below the accepted range
    emitter.instruction("cmp r8b, 90");                                         // test for the upper bound of uppercase base64 letters
    emitter.instruction("jg __rt_asf_b64_try_lower_x86");                       // reject values above the accepted range
    emitter.instruction("sub r8b, 65");                                         // convert a base64 letter into its sextet value
    emitter.instruction("jmp __rt_asf_b64_add_x86");                            // continue at __rt_asf_b64_add_x86
    emitter.label("__rt_asf_b64_try_lower_x86");
    emitter.instruction("cmp r8b, 97");                                         // test for the lower bound of lowercase hex/base64 letters
    emitter.instruction("jl __rt_asf_b64_try_plus_x86");                        // reject values below the accepted range
    emitter.instruction("cmp r8b, 122");                                        // test for the upper bound of lowercase base64 letters
    emitter.instruction("jg __rt_asf_b64_try_plus_x86");                        // reject values above the accepted range
    emitter.instruction("sub r8b, 71");                                         // convert a base64 letter into its sextet value
    emitter.instruction("jmp __rt_asf_b64_add_x86");                            // continue at __rt_asf_b64_add_x86
    emitter.label("__rt_asf_b64_try_digit_x86");
    emitter.instruction("cmp r8b, 48");                                         // test for the lower bound of ASCII digits
    emitter.instruction("jl __rt_asf_b64_try_plus_x86");                        // reject values below the accepted range
    emitter.instruction("cmp r8b, 57");                                         // test for the upper bound of ASCII digits
    emitter.instruction("jg __rt_asf_b64_try_plus_x86");                        // reject values above the accepted range
    emitter.instruction("add r8b, 4");                                          // convert an ASCII digit into its base64 sextet
    emitter.instruction("jmp __rt_asf_b64_add_x86");                            // continue at __rt_asf_b64_add_x86
    emitter.label("__rt_asf_b64_try_plus_x86");
    emitter.instruction("cmp r8b, 43");                                         // test for '+' in the base64 alphabet
    emitter.instruction("je __rt_asf_b64_plus_x86");                            // map '+' to its base64 sextet
    emitter.instruction("cmp r8b, 47");                                         // test for '/' in the base64 alphabet
    emitter.instruction("je __rt_asf_b64_slash_x86");                           // map '/' to its base64 sextet
    emitter.instruction("jmp __rt_asf_b64_loop_x86");                           // skip non-base64
    emitter.label("__rt_asf_b64_plus_x86");
    emitter.instruction("mov r8b, 62");                                         // map '+' to base64 value 62
    emitter.instruction("jmp __rt_asf_b64_add_x86");                            // continue at __rt_asf_b64_add_x86
    emitter.label("__rt_asf_b64_slash_x86");
    emitter.instruction("mov r8b, 63");                                         // map '/' to base64 value 63
    emitter.label("__rt_asf_b64_add_x86");
    emitter.instruction("shl r11, 6");                                          // shift the accumulator to make room for the next value
    emitter.instruction("movzx r8, r8b");                                       // zero-extend the decoded base64 sextet
    emitter.instruction("or r11, r8");                                          // merge the extracted bits into the accumulator
    emitter.instruction("inc r12");                                             // advance the base64 group count
    emitter.instruction("cmp r12, 4");                                          // check whether a full four-character group is ready
    emitter.instruction("jl __rt_asf_b64_loop_x86");                            // reject values below the accepted range
    // Emit 3 bytes.
    emitter.instruction("mov r13, r11");                                        // stage bits before extracting the next output byte
    emitter.instruction("shr r13, 16");                                         // extract the next sextet or byte from the accumulated bits
    emitter.instruction("mov BYTE PTR [rax + r10], r13b");                      // write output back into the stream buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("mov r13, r11");                                        // stage bits before extracting the next output byte
    emitter.instruction("shr r13, 8");                                          // extract the next sextet or byte from the accumulated bits
    emitter.instruction("mov BYTE PTR [rax + r10], r13b");                      // write output back into the stream buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("mov BYTE PTR [rax + r10], r11b");                      // write output back into the stream buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("xor r11, r11");                                        // initialize the filter accumulator or state flag
    emitter.instruction("xor r12, r12");                                        // initialize the base64 group count
    emitter.instruction("jmp __rt_asf_b64_loop_x86");                           // continue the base64 decoder loop
    emitter.label("__rt_asf_b64_done_x86");
    // Partial group handling.
    emitter.instruction("cmp r12, 2");                                          // check whether a partial group can produce output
    emitter.instruction("jl __rt_asf_b64_finish_x86");                          // reject values below the accepted range
    emitter.instruction("cmp r12, 3");                                          // check whether a full three-byte group is available
    emitter.instruction("je __rt_asf_b64_three_x86");                           // dispatch to __rt_asf_b64_three_x86 when the comparison matches
    // 2 chars.
    emitter.instruction("shl r11, 12");                                         // shift the accumulator to make room for the next value
    emitter.instruction("mov r13, r11");                                        // stage bits before extracting the next output byte
    emitter.instruction("shr r13, 16");                                         // extract the next sextet or byte from the accumulated bits
    emitter.instruction("mov BYTE PTR [rax + r10], r13b");                      // write output back into the stream buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("jmp __rt_asf_b64_finish_x86");                         // continue at __rt_asf_b64_finish_x86
    emitter.label("__rt_asf_b64_three_x86");
    emitter.instruction("shl r11, 6");                                          // shift the accumulator to make room for the next value
    emitter.instruction("mov r13, r11");                                        // stage bits before extracting the next output byte
    emitter.instruction("shr r13, 16");                                         // extract the next sextet or byte from the accumulated bits
    emitter.instruction("mov BYTE PTR [rax + r10], r13b");                      // write output back into the stream buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("mov r13, r11");                                        // stage bits before extracting the next output byte
    emitter.instruction("shr r13, 8");                                          // extract the next sextet or byte from the accumulated bits
    emitter.instruction("mov BYTE PTR [rax + r10], r13b");                      // write output back into the stream buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.label("__rt_asf_b64_finish_x86");
    emitter.instruction("mov rdx, r10");                                        // return the transformed output length
    emitter.instruction("ret");                                                 // return to the stream-filter caller

    // -- dechunk (x86_64) — php-src's `php_dechunk`; the AArch64 arm carries the rules.
    //    r9 = read cursor, r10 = write cursor, r11 = chunk size, r13 = the size-line start and,
    //    once the size line is behind us, the byte count of the body copy. --
    emitter.label("__rt_asf_dechunk_x86");
    emitter.instruction("xor r9, r9");                                          // read index
    emitter.instruction("xor r10, r10");                                        // write index
    emitter.label("__rt_asf_dc_size_start_x86");
    emitter.instruction("cmp r9, rdx");                                         // php's outer `while (p < end)`
    emitter.instruction("jge __rt_asf_dc_done_x86");                            // nothing left to parse
    emitter.instruction("xor r11, r11");                                        // chunk size accumulator
    emitter.instruction("mov r13, r9");                                         // remember where the size line began
    emitter.label("__rt_asf_dc_size_scan_x86");
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_done_x86");                            // a size line cut short yields what was decoded
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("cmp r8b, 48");                                         // test for the lower bound of ASCII digits
    emitter.instruction("jl __rt_asf_dc_size_nonhex_x86");                      // not a hex digit: the size line is over
    emitter.instruction("cmp r8b, 57");                                         // test for the upper bound of ASCII digits
    emitter.instruction("jle __rt_asf_dc_size_digit_x86");                      // 0..9
    emitter.instruction("or r8b, 0x20");                                        // case-fold A..F onto a..f
    emitter.instruction("cmp r8b, 97");                                         // test for the lower bound of lowercase hex letters
    emitter.instruction("jl __rt_asf_dc_size_nonhex_x86");                      // not a hex digit: the size line is over
    emitter.instruction("cmp r8b, 102");                                        // test for the upper bound of lowercase hex letters
    emitter.instruction("jg __rt_asf_dc_size_nonhex_x86");                      // not a hex digit: the size line is over
    emitter.instruction("sub r8b, 87");                                         // a..f → 10..15
    emitter.instruction("jmp __rt_asf_dc_size_acc_x86");                        // accumulate the digit
    emitter.label("__rt_asf_dc_size_digit_x86");
    emitter.instruction("sub r8b, 48");                                         // convert an ASCII digit into its numeric value
    emitter.label("__rt_asf_dc_size_acc_x86");
    emitter.instruction("shl r11, 4");                                          // shift the accumulator to make room for the next value
    emitter.instruction("movzx r8, r8b");                                       // zero-extend the parsed chunk-size digit
    emitter.instruction("or r11, r8");                                          // merge the extracted bits into the accumulator
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("jmp __rt_asf_dc_size_scan_x86");                       // continue at __rt_asf_dc_size_scan_x86
    emitter.label("__rt_asf_dc_size_nonhex_x86");
    emitter.instruction("cmp r9, r13");                                         // did the line hold ANY hex digit?
    emitter.instruction("je __rt_asf_dc_error_x86");                            // php's CHUNK_ERROR: a size line that starts non-hex
    emitter.label("__rt_asf_dc_ext_x86");
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_done_x86");                            // finish the dechunk operation when the bound is reached
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("cmp r8b, 13");                                         // test for carriage return in the encoded stream
    emitter.instruction("je __rt_asf_dc_size_cr_x86");                          // the size line ends here
    emitter.instruction("cmp r8b, 10");                                         // test for line feed in the encoded stream
    emitter.instruction("je __rt_asf_dc_size_cr_x86");                          // the size line ends here
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("jmp __rt_asf_dc_ext_x86");                             // continue skipping the extension
    emitter.label("__rt_asf_dc_size_cr_x86");
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("cmp r8b, 13");                                         // test for carriage return in the encoded stream
    emitter.instruction("jne __rt_asf_dc_size_lf_x86");                         // no CR: the LF is judged on this same byte
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_done_x86");                            // finish the dechunk operation when the bound is reached
    emitter.label("__rt_asf_dc_size_lf_x86");
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("cmp r8b, 10");                                         // test for line feed in the encoded stream
    emitter.instruction("jne __rt_asf_dc_error_x86");                           // php's CHUNK_ERROR
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("test r11, r11");                                       // size 0 is the last chunk
    emitter.instruction("jz __rt_asf_dc_done_x86");                             // php ignores the trailer
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_done_x86");                            // finish the dechunk operation when the bound is reached
    emitter.instruction("mov r13, rdx");                                        // bytes still in the buffer
    emitter.instruction("sub r13, r9");                                         // end - p
    emitter.instruction("cmp r13, r11");                                        // is the whole chunk here?
    emitter.instruction("jl __rt_asf_dc_body_copy_x86");                        // a partial chunk copies what arrived
    emitter.instruction("mov r13, r11");                                        // the whole chunk
    emitter.label("__rt_asf_dc_body_copy_x86");
    emitter.instruction("xor r12, r12");                                        // chunk-copy cursor
    emitter.label("__rt_asf_dc_copy_loop_x86");
    emitter.instruction("cmp r12, r13");                                        // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_copy_done_x86");                       // the chunk body is copied
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("mov BYTE PTR [rax + r10], r8b");                       // write output back into the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("inc r12");                                             // advance the chunk-copy cursor
    emitter.instruction("jmp __rt_asf_dc_copy_loop_x86");                       // continue the dechunk loop
    emitter.label("__rt_asf_dc_copy_done_x86");
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_done_x86");                            // finish the dechunk operation when the bound is reached
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("cmp r8b, 13");                                         // test for carriage return in the encoded stream
    emitter.instruction("jne __rt_asf_dc_body_lf_x86");                         // no CR: the LF is judged on this same byte
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_done_x86");                            // finish the dechunk operation when the bound is reached
    emitter.label("__rt_asf_dc_body_lf_x86");
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("cmp r8b, 10");                                         // test for line feed in the encoded stream
    emitter.instruction("jne __rt_asf_dc_error_x86");                           // php's CHUNK_ERROR
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("jmp __rt_asf_dc_size_start_x86");                      // the next chunk-size line
    emitter.label("__rt_asf_dc_error_x86");
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_done_x86");                            // finish the dechunk operation when the bound is reached
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("mov BYTE PTR [rax + r10], r8b");                       // write output back into the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("jmp __rt_asf_dc_error_x86");                           // continue copying the remainder
    emitter.label("__rt_asf_dc_done_x86");
    emitter.instruction("mov rdx, r10");                                        // return the transformed output length
    emitter.instruction("ret");                                                 // return to the stream-filter caller

    // -- convert.quoted-printable-decode (x86_64) --
    emitter.label("__rt_asf_qp_decode_x86");
    emitter.instruction("xor r9, r9");                                          // read index
    emitter.instruction("xor r10, r10");                                        // write index
    emitter.label("__rt_asf_qp_loop_x86");
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_qp_done_x86");                            // finish the quoted-printable decoder operation when the bound is reached
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("cmp r8b, 61");                                         // test for '=' before quoted-printable escaping
    emitter.instruction("je __rt_asf_qp_escape_x86");                           // escape the current byte
    emitter.instruction("mov BYTE PTR [rax + r10], r8b");                       // write output back into the stream buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("jmp __rt_asf_qp_loop_x86");                            // continue the quoted-printable decoder loop
    emitter.label("__rt_asf_qp_escape_x86");
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_qp_done_x86");                            // finish the quoted-printable decoder operation when the bound is reached
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("cmp r8b, 13");                                         // test for carriage return in the encoded stream
    emitter.instruction("je __rt_asf_qp_soft_x86");                             // handle a quoted-printable soft line break
    emitter.instruction("cmp r8b, 10");                                         // test for line feed in the encoded stream
    emitter.instruction("je __rt_asf_qp_soft_lf_x86");                          // handle a quoted-printable soft line break
    // hi nibble inlined
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("mov r11d, -1");                                        // mark the hex nibble as invalid until classification succeeds
    emitter.instruction("cmp r8b, 48");                                         // test for the lower bound of ASCII digits
    emitter.instruction("jl __rt_asf_qp_hi_alpha_x86");                         // reject values below the accepted range
    emitter.instruction("cmp r8b, 57");                                         // test for the upper bound of ASCII digits
    emitter.instruction("jg __rt_asf_qp_hi_alpha_x86");                         // reject values above the accepted range
    emitter.instruction("movzx r11, r8b");                                      // zero-extend the high hex digit
    emitter.instruction("sub r11, 48");                                         // convert an ASCII digit into its numeric value
    emitter.instruction("jmp __rt_asf_qp_hi_done_x86");                         // continue at __rt_asf_qp_hi_done_x86
    emitter.label("__rt_asf_qp_hi_alpha_x86");
    emitter.instruction("or r8b, 0x20");                                        // merge the extracted bits into the accumulator
    emitter.instruction("cmp r8b, 97");                                         // test for the lower bound of lowercase hex/base64 letters
    emitter.instruction("jl __rt_asf_qp_hi_done_x86");                          // reject values below the accepted range
    emitter.instruction("cmp r8b, 102");                                        // test for line feed in the encoded stream
    emitter.instruction("jg __rt_asf_qp_hi_done_x86");                          // reject values above the accepted range
    emitter.instruction("movzx r11, r8b");                                      // zero-extend the high hex letter
    emitter.instruction("sub r11, 87");                                         // convert a lowercase hex letter into its numeric value
    emitter.label("__rt_asf_qp_hi_done_x86");
    emitter.instruction("cmp r11d, 0");                                         // check whether the current cursor reached its bound
    emitter.instruction("jl __rt_asf_qp_loop_x86");                             // reject values below the accepted range
    emitter.instruction("mov r12, r11");                                        // hi nibble
    // lo nibble
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_qp_done_x86");                            // finish the quoted-printable decoder operation when the bound is reached
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("mov r11d, -1");                                        // mark the hex nibble as invalid until classification succeeds
    emitter.instruction("cmp r8b, 48");                                         // test for the lower bound of ASCII digits
    emitter.instruction("jl __rt_asf_qp_lo_alpha_x86");                         // reject values below the accepted range
    emitter.instruction("cmp r8b, 57");                                         // test for the upper bound of ASCII digits
    emitter.instruction("jg __rt_asf_qp_lo_alpha_x86");                         // reject values above the accepted range
    emitter.instruction("movzx r11, r8b");                                      // zero-extend the low hex digit
    emitter.instruction("sub r11, 48");                                         // convert an ASCII digit into its numeric value
    emitter.instruction("jmp __rt_asf_qp_lo_done_x86");                         // continue at __rt_asf_qp_lo_done_x86
    emitter.label("__rt_asf_qp_lo_alpha_x86");
    emitter.instruction("or r8b, 0x20");                                        // merge the extracted bits into the accumulator
    emitter.instruction("cmp r8b, 97");                                         // test for the lower bound of lowercase hex/base64 letters
    emitter.instruction("jl __rt_asf_qp_lo_done_x86");                          // reject values below the accepted range
    emitter.instruction("cmp r8b, 102");                                        // test for line feed in the encoded stream
    emitter.instruction("jg __rt_asf_qp_lo_done_x86");                          // reject values above the accepted range
    emitter.instruction("movzx r11, r8b");                                      // zero-extend the low hex letter
    emitter.instruction("sub r11, 87");                                         // convert a lowercase hex letter into its numeric value
    emitter.label("__rt_asf_qp_lo_done_x86");
    emitter.instruction("cmp r11d, 0");                                         // check whether the current cursor reached its bound
    emitter.instruction("jl __rt_asf_qp_loop_x86");                             // reject values below the accepted range
    emitter.instruction("shl r12, 4");                                          // move the high nibble into byte position
    emitter.instruction("or r12, r11");                                         // merge the extracted bits into the accumulator
    emitter.instruction("mov BYTE PTR [rax + r10], r12b");                      // write output back into the stream buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("jmp __rt_asf_qp_loop_x86");                            // continue the quoted-printable decoder loop
    emitter.label("__rt_asf_qp_soft_x86");
    emitter.instruction("inc r9");                                              // skip \r
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_qp_loop_x86");                            // leave the loop when the cursor reaches its bound
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("cmp r8b, 10");                                         // test for line feed in the encoded stream
    emitter.instruction("jne __rt_asf_qp_loop_x86");                            // continue the quoted-printable decoder loop when the delimiter was not found
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("jmp __rt_asf_qp_loop_x86");                            // continue the quoted-printable decoder loop
    emitter.label("__rt_asf_qp_soft_lf_x86");
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("jmp __rt_asf_qp_loop_x86");                            // continue the quoted-printable decoder loop
    emitter.label("__rt_asf_qp_done_x86");
    emitter.instruction("mov rdx, r10");                                        // return the transformed output length
    emitter.instruction("ret");                                                 // return to the stream-filter caller

    // -- convert.base64-encode (x86_64) --
    emitter.label("__rt_asf_b64_encode_x86");
    // Cap input at 49152 bytes so the 4/3 expansion fits the scratch buffer.
    emitter.instruction("mov r11, 49152");                                      // set the maximum input length that fits the scratch buffer
    emitter.instruction("cmp rdx, r11");                                        // check whether the current cursor reached its bound
    emitter.instruction("cmovg rdx, r11");                                      // rdx = MIN(rdx, 49152)
    abi::emit_symbol_address(emitter, "r11", "_stream_grow_scratch");           // r11 = scratch base
    abi::emit_symbol_address(emitter, "r12", "_b64_encode_tbl");                // r12 = alphabet table
    emitter.instruction("xor r9, r9");                                          // read idx
    emitter.instruction("xor r10, r10");                                        // write idx
    emitter.label("__rt_asf_b64e_loop_x86");
    emitter.instruction("mov rcx, rdx");                                        // stage bits before extracting the next output byte
    emitter.instruction("sub rcx, r9");                                         // remaining bytes
    emitter.instruction("cmp rcx, 3");                                          // check whether a full three-byte group is available
    emitter.instruction("jl __rt_asf_b64e_rem_x86");                            // reject values below the accepted range
    // Read 3 bytes.
    emitter.instruction("movzx r13d, BYTE PTR [rax + r9]");                     // byte 0
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("movzx r14d, BYTE PTR [rax + r9]");                     // byte 1
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("movzx r15d, BYTE PTR [rax + r9]");                     // byte 2
    emitter.instruction("inc r9");                                              // advance the read cursor
    // char 0: b0 >> 2
    emitter.instruction("mov rcx, r13");                                        // stage bits before extracting the next output byte
    emitter.instruction("shr rcx, 2");                                          // extract the next sextet or byte from the accumulated bits
    emitter.instruction("movzx ecx, BYTE PTR [r12 + rcx]");                     // load the base64 alphabet byte for the sextet
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // write encoded output into the scratch buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    // char 1: ((b0 & 3) << 4) | (b1 >> 4)
    emitter.instruction("mov rcx, r13");                                        // stage bits before extracting the next output byte
    emitter.instruction("and rcx, 3");                                          // mask the bits needed for the next encoded byte
    emitter.instruction("shl rcx, 4");                                          // shift bits into the position required by the output byte
    emitter.instruction("mov r8, r14");                                         // stage bits before extracting the next output byte
    emitter.instruction("shr r8, 4");                                           // extract the next sextet or byte from the accumulated bits
    emitter.instruction("or rcx, r8");                                          // merge the extracted bits into the accumulator
    emitter.instruction("movzx ecx, BYTE PTR [r12 + rcx]");                     // load the base64 alphabet byte for the sextet
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // write encoded output into the scratch buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    // char 2: ((b1 & 15) << 2) | (b2 >> 6)
    emitter.instruction("mov rcx, r14");                                        // stage bits before extracting the next output byte
    emitter.instruction("and rcx, 15");                                         // mask the bits needed for the next encoded byte
    emitter.instruction("shl rcx, 2");                                          // shift bits into the position required by the output byte
    emitter.instruction("mov r8, r15");                                         // stage bits before extracting the next output byte
    emitter.instruction("shr r8, 6");                                           // extract the next sextet or byte from the accumulated bits
    emitter.instruction("or rcx, r8");                                          // merge the extracted bits into the accumulator
    emitter.instruction("movzx ecx, BYTE PTR [r12 + rcx]");                     // load the base64 alphabet byte for the sextet
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // write encoded output into the scratch buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    // char 3: b2 & 0x3f
    emitter.instruction("mov rcx, r15");                                        // stage bits before extracting the next output byte
    emitter.instruction("and rcx, 63");                                         // mask the bits needed for the next encoded byte
    emitter.instruction("movzx ecx, BYTE PTR [r12 + rcx]");                     // load the base64 alphabet byte for the sextet
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // write encoded output into the scratch buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("jmp __rt_asf_b64e_loop_x86");                          // continue the base64 encoder loop
    emitter.label("__rt_asf_b64e_rem_x86");
    emitter.instruction("test rcx, rcx");                                       // check whether the current counter or flag is zero
    emitter.instruction("jz __rt_asf_b64e_copyback_x86");                       // finish the base64 encoder operation when the count is zero
    emitter.instruction("cmp rcx, 1");                                          // check whether only one byte remains
    emitter.instruction("je __rt_asf_b64e_rem1_x86");                           // handle a one-byte base64 tail
    // 2-byte tail: 3 chars + '='
    emitter.instruction("movzx r13d, BYTE PTR [rax + r9]");                     // load the next byte from the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("movzx r14d, BYTE PTR [rax + r9]");                     // load the next byte from the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("mov rcx, r13");                                        // stage bits before extracting the next output byte
    emitter.instruction("shr rcx, 2");                                          // extract the next sextet or byte from the accumulated bits
    emitter.instruction("movzx ecx, BYTE PTR [r12 + rcx]");                     // load the base64 alphabet byte for the sextet
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // write encoded output into the scratch buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("mov rcx, r13");                                        // stage bits before extracting the next output byte
    emitter.instruction("and rcx, 3");                                          // mask the bits needed for the next encoded byte
    emitter.instruction("shl rcx, 4");                                          // shift bits into the position required by the output byte
    emitter.instruction("mov r8, r14");                                         // stage bits before extracting the next output byte
    emitter.instruction("shr r8, 4");                                           // extract the next sextet or byte from the accumulated bits
    emitter.instruction("or rcx, r8");                                          // merge the extracted bits into the accumulator
    emitter.instruction("movzx ecx, BYTE PTR [r12 + rcx]");                     // load the base64 alphabet byte for the sextet
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // write encoded output into the scratch buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("mov rcx, r14");                                        // stage bits before extracting the next output byte
    emitter.instruction("and rcx, 15");                                         // mask the bits needed for the next encoded byte
    emitter.instruction("shl rcx, 2");                                          // shift bits into the position required by the output byte
    emitter.instruction("movzx ecx, BYTE PTR [r12 + rcx]");                     // load the base64 alphabet byte for the sextet
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // write encoded output into the scratch buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("mov BYTE PTR [r11 + r10], 61");                        // '='
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("jmp __rt_asf_b64e_copyback_x86");                      // copy the scratch output back into the stream buffer
    emitter.label("__rt_asf_b64e_rem1_x86");
    // 1-byte tail: 2 chars + '=='
    emitter.instruction("movzx r13d, BYTE PTR [rax + r9]");                     // load the next byte from the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("mov rcx, r13");                                        // stage bits before extracting the next output byte
    emitter.instruction("shr rcx, 2");                                          // extract the next sextet or byte from the accumulated bits
    emitter.instruction("movzx ecx, BYTE PTR [r12 + rcx]");                     // load the base64 alphabet byte for the sextet
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // write encoded output into the scratch buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("mov rcx, r13");                                        // stage bits before extracting the next output byte
    emitter.instruction("and rcx, 3");                                          // mask the bits needed for the next encoded byte
    emitter.instruction("shl rcx, 4");                                          // shift bits into the position required by the output byte
    emitter.instruction("movzx ecx, BYTE PTR [r12 + rcx]");                     // load the base64 alphabet byte for the sextet
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // write encoded output into the scratch buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("mov BYTE PTR [r11 + r10], 61");                        // '='
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("mov BYTE PTR [r11 + r10], 61");                        // '='
    emitter.instruction("inc r10");                                             // advance the write cursor
    // See the AArch64 counterpart: php wraps base64 output at `line-length`, with
    // `line-break-chars` (default CRLF) between lines and no trailing break. A zero length — php's
    // default, and what a filter attached without `$params` leaves on the node — takes the plain
    // copy, so the common case pays one compare.
    //
    // `rbx` and `r12` are borrowed for the break emission: this helper is a leaf with no frame, and
    // the wrapping copy needs two cursors more than the encoder left free.
    emitter.label("__rt_asf_b64e_copyback_x86");
    emitter.instruction("push rbx");                                            // borrowed, restored at the single exit
    emitter.instruction("push r12");
    abi::emit_symbol_address(emitter, "rdi", "_asf_line_length");
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // the width the caller asked for
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_asf_b64e_cb_plain_x86");                       // php wraps at nothing by default
    emitter.instruction("xor r9, r9");                                          // scratch read cursor
    emitter.instruction("xor rsi, rsi");                                        // buffer write cursor
    emitter.instruction("xor r8, r8");                                          // characters on the current line
    emitter.label("__rt_asf_b64e_cb_wloop_x86");
    emitter.instruction("cmp r9, r10");                                         // copied every encoded byte?
    emitter.instruction("jge __rt_asf_b64e_cb_fin_x86");
    emitter.instruction("cmp r8, rdi");                                         // is the current line full?
    emitter.instruction("jl __rt_asf_b64e_cb_nobreak_x86");                     // no: keep filling it
    abi::emit_symbol_address(emitter, "rcx", "_asf_break_ptr");
    emitter.instruction("mov rcx, QWORD PTR [rcx]");                            // the caller's break bytes
    abi::emit_symbol_address(emitter, "rbx", "_asf_break_len");
    emitter.instruction("mov rbx, QWORD PTR [rbx]");
    emitter.instruction("test rcx, rcx");
    emitter.instruction("jnz __rt_asf_b64e_cb_have_x86");
    abi::emit_symbol_address(emitter, "rcx", "_filter_break_crlf");
    emitter.instruction("mov rbx, 2");                                          // php's default break is CRLF
    emitter.label("__rt_asf_b64e_cb_have_x86");
    emitter.instruction("xor r12, r12");                                        // break-byte cursor
    emitter.label("__rt_asf_b64e_cb_bloop_x86");
    emitter.instruction("cmp r12, rbx");                                        // written the whole break?
    emitter.instruction("jge __rt_asf_b64e_cb_bend_x86");
    emitter.instruction("movzx edx, BYTE PTR [rcx + r12]");                     // one break byte
    emitter.instruction("mov BYTE PTR [rax + rsi], dl");
    emitter.instruction("inc rsi");
    emitter.instruction("inc r12");
    emitter.instruction("jmp __rt_asf_b64e_cb_bloop_x86");
    emitter.label("__rt_asf_b64e_cb_bend_x86");
    emitter.instruction("xor r8, r8");                                          // the new line starts empty
    emitter.label("__rt_asf_b64e_cb_nobreak_x86");
    emitter.instruction("movzx edx, BYTE PTR [r11 + r9]");                      // one encoded byte
    emitter.instruction("mov BYTE PTR [rax + rsi], dl");
    emitter.instruction("inc r9");
    emitter.instruction("inc rsi");
    emitter.instruction("inc r8");
    emitter.instruction("jmp __rt_asf_b64e_cb_wloop_x86");

    emitter.label("__rt_asf_b64e_cb_plain_x86");
    emitter.instruction("xor r9, r9");                                          // initialize the read cursor
    emitter.instruction("mov rsi, r10");                                        // the written length equals the encoded one
    emitter.label("__rt_asf_b64e_cb_loop_x86");
    emitter.instruction("cmp r9, r10");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_b64e_cb_fin_x86");                        // finish when the bound is reached
    emitter.instruction("movzx ecx, BYTE PTR [r11 + r9]");                      // load the next byte from the scratch buffer
    emitter.instruction("mov BYTE PTR [rax + r9], cl");                         // copy scratch output back into the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("jmp __rt_asf_b64e_cb_loop_x86");                       // continue the base64 encoder loop

    emitter.label("__rt_asf_b64e_cb_fin_x86");
    emitter.label("__rt_asf_b64e_done_x86");
    emitter.instruction("mov rdx, rsi");                                        // return what was WRITTEN, breaks included
    emitter.instruction("pop r12");
    emitter.instruction("pop rbx");
    emitter.instruction("ret");                                                 // return to the stream-filter caller

    // -- convert.quoted-printable-encode (x86_64): php's `php_conv_qprint_encode_convert`,
    //    the same algorithm the AArch64 arm above runs. See it for the three rules and the
    //    measurements they came from. --
    emitter.label("__rt_asf_qp_encode_x86");
    emitter.instruction("push rbx");                                                                    // lbchars pointer
    emitter.instruction("push r12");                                                                    // lbchars length
    emitter.instruction("push r13");                                                                    // lb_cnt
    emitter.instruction("push r14");                                                                    // lb_ptr
    emitter.instruction("push r15");                                                                    // trail_ws
    // Three values with no register left. rsp does not move inside the loop and this arm makes
    // no calls, so the slots are stable: [rsp] `binary`, [rsp+8] the current byte across the
    // whitespace look-ahead, [rsp+16] lb_cnt across the same scan.
    emitter.instruction("sub rsp, 32");                                                                 // spill area for binary, the current byte and lb_cnt
    emitter.instruction("mov r11, 21845");                                                              // ~64KB/3 worst-case cap
    emitter.instruction("cmp rdx, r11");
    emitter.instruction("cmovg rdx, r11");                                                              // rdx = MIN(rdx, 21845)

    // -- the three parameters, read ONCE --
    abi::emit_symbol_address(emitter, "rcx", "_asf_binary");
    emitter.instruction("mov rcx, QWORD PTR [rcx]");
    emitter.instruction("mov QWORD PTR [rsp], rcx");                                                    // `binary` for the rest of the body
    abi::emit_symbol_address(emitter, "rdi", "_asf_line_length");
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                                                    // the width php wraps at
    // php's factory: `line-length < 4` nulls the break chars, turning OFF wrapping, the
    // line-break passthrough and the trailing-whitespace rule together.
    emitter.instruction("cmp rdi, 4");
    emitter.instruction("jge __rt_asf_qpe_lb_on_x86");
    emitter.instruction("xor rdi, rdi");                                                                // no wrapping
    emitter.instruction("xor rbx, rbx");                                                                // and no break chars
    emitter.instruction("xor r12, r12");
    emitter.instruction("jmp __rt_asf_qpe_lb_ready_x86");
    emitter.label("__rt_asf_qpe_lb_on_x86");
    abi::emit_symbol_address(emitter, "rbx", "_asf_break_ptr");
    emitter.instruction("mov rbx, QWORD PTR [rbx]");                                                    // the caller's break bytes
    abi::emit_symbol_address(emitter, "r12", "_asf_break_len");
    emitter.instruction("mov r12, QWORD PTR [r12]");
    emitter.instruction("test rbx, rbx");
    emitter.instruction("jnz __rt_asf_qpe_lb_ready_x86");                                               // the caller named its own
    abi::emit_symbol_address(emitter, "rbx", "_filter_break_crlf");
    emitter.instruction("mov r12, 2");                                                                  // php's default break is CRLF
    emitter.label("__rt_asf_qpe_lb_ready_x86");
    // The `=` of a soft break needs the last column, so every wrap test compares against
    // `line_len - 1`. Precomputing it here is what keeps the tests from borrowing a live
    // register for the arithmetic — which cost two clobbered values in the first draft.
    emitter.instruction("test rbx, rbx");
    emitter.instruction("jz __rt_asf_qpe_limit_ready_x86");
    emitter.instruction("dec rdi");
    emitter.label("__rt_asf_qpe_limit_ready_x86");
    abi::emit_symbol_address(emitter, "r11", "_stream_grow_scratch");
    emitter.instruction("xor r9, r9");                                                                  // read index
    emitter.instruction("xor r10, r10");                                                                // write index
    emitter.instruction("xor rsi, rsi");                                                                // columns used on the current line
    emitter.instruction("xor r13, r13");                                                                // lb_cnt
    emitter.instruction("xor r14, r14");                                                                // lb_ptr
    emitter.instruction("xor r15, r15");                                                                // trail_ws

    emitter.label("__rt_asf_qpe_loop_x86");
    emitter.instruction("cmp r10, 65500");
    emitter.instruction("jge __rt_asf_qpe_copyback_x86");                                               // never write past the scratch
    // -- the line-break lookahead --
    emitter.instruction("cmp QWORD PTR [rsp], 0");
    emitter.instruction("jne __rt_asf_qpe_no_lb_x86");                                                  // binary: the sequence is not special
    emitter.instruction("test rbx, rbx");
    emitter.instruction("jz __rt_asf_qpe_no_lb_x86");                                                   // no break chars in force
    emitter.instruction("test r12, r12");
    emitter.instruction("jz __rt_asf_qpe_no_lb_x86");                                                   // a zero-length break matches nothing
    emitter.instruction("cmp r9, rdx");
    emitter.instruction("jge __rt_asf_qpe_no_lb_x86");                                                  // nothing left to look at
    emitter.instruction("movzx ecx, BYTE PTR [rax + r9]");                                              // the byte under the cursor
    emitter.instruction("cmp cl, BYTE PTR [rbx + r13]");                                                // the break byte it has to match
    emitter.instruction("jne __rt_asf_qpe_no_lb_x86");
    emitter.instruction("inc r13");                                                                     // one more byte of the sequence matched
    emitter.instruction("inc r9");
    emitter.instruction("cmp r13, r12");
    emitter.instruction("jl __rt_asf_qpe_loop_x86");                                                    // a partial match: keep looking
    // The whole sequence: php copies it out VERBATIM and starts a fresh line.
    emitter.instruction("xor rcx, rcx");
    emitter.label("__rt_asf_qpe_lb_emit_x86");
    emitter.instruction("cmp rcx, r13");
    emitter.instruction("jge __rt_asf_qpe_lb_emitted_x86");
    emitter.instruction("movzx r8d, BYTE PTR [rbx + rcx]");
    emitter.instruction("mov BYTE PTR [r11 + r10], r8b");
    emitter.instruction("inc r10");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_asf_qpe_lb_emit_x86");
    emitter.label("__rt_asf_qpe_lb_emitted_x86");
    emitter.instruction("xor rsi, rsi");                                                                // the break restarts the line budget
    emitter.instruction("xor r13, r13");
    emitter.instruction("xor r14, r14");
    emitter.instruction("jmp __rt_asf_qpe_loop_x86");

    emitter.label("__rt_asf_qpe_no_lb_x86");
    // php: `if (lb_ptr >= lb_cnt && icnt == 0) break;`
    emitter.instruction("cmp r14, r13");
    emitter.instruction("jl __rt_asf_qpe_have_x86");
    emitter.instruction("cmp r9, rdx");
    emitter.instruction("jge __rt_asf_qpe_copyback_x86");
    emitter.label("__rt_asf_qpe_have_x86");
    emitter.instruction("cmp r14, r13");
    emitter.instruction("jge __rt_asf_qpe_from_input_x86");
    emitter.instruction("movzx r8d, BYTE PTR [rbx + r14]");                                             // replay a byte of the match that broke off
    emitter.instruction("jmp __rt_asf_qpe_classify_x86");
    emitter.label("__rt_asf_qpe_from_input_x86");
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");
    emitter.label("__rt_asf_qpe_classify_x86");
    emitter.instruction("cmp QWORD PTR [rsp], 0");
    emitter.instruction("jne __rt_asf_qpe_range_x86");                                                  // binary escapes SPACE and TAB like any other byte
    emitter.instruction("test r15, r15");
    emitter.instruction("jnz __rt_asf_qpe_range_x86");                                                  // a line-ending run already decided: escape
    emitter.instruction("cmp r8b, 9");
    emitter.instruction("je __rt_asf_qpe_ws_x86");
    emitter.instruction("cmp r8b, 32");
    emitter.instruction("je __rt_asf_qpe_ws_x86");
    emitter.label("__rt_asf_qpe_range_x86");
    emitter.instruction("cmp r8b, 33");
    emitter.instruction("jl __rt_asf_qpe_escape_x86");
    emitter.instruction("cmp r8b, 126");
    emitter.instruction("jg __rt_asf_qpe_escape_x86");
    emitter.instruction("cmp r8b, 61");                                                                 // '=' is escaped even though it is printable
    emitter.instruction("je __rt_asf_qpe_escape_x86");

    // -- a printable byte: one column --
    emitter.instruction("test rbx, rbx");
    emitter.instruction("jz __rt_asf_qpe_lit_fits_x86");                                                // no break chars means no wrapping
    emitter.instruction("lea rcx, [rsi + 1]");
    emitter.instruction("cmp rcx, rdi");
    emitter.instruction("jle __rt_asf_qpe_lit_fits_x86");
    emitter.instruction("mov BYTE PTR [r11 + r10], 61");
    emitter.instruction("inc r10");
    emitter.instruction("xor rcx, rcx");
    emitter.label("__rt_asf_qpe_lit_bloop_x86");
    emitter.instruction("cmp rcx, r12");
    emitter.instruction("jge __rt_asf_qpe_lit_bend_x86");
    emitter.instruction("mov r8b, BYTE PTR [rbx + rcx]");
    emitter.instruction("mov BYTE PTR [r11 + r10], r8b");
    emitter.instruction("inc r10");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_asf_qpe_lit_bloop_x86");
    emitter.label("__rt_asf_qpe_lit_bend_x86");
    emitter.instruction("xor rsi, rsi");
    // The break loop borrowed r8b for its bytes; re-read the one being encoded.
    emitter.instruction("cmp r14, r13");
    emitter.instruction("jge __rt_asf_qpe_lit_from_input_x86");
    emitter.instruction("movzx r8d, BYTE PTR [rbx + r14]");
    emitter.instruction("jmp __rt_asf_qpe_lit_fits_x86");
    emitter.label("__rt_asf_qpe_lit_from_input_x86");
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");
    emitter.label("__rt_asf_qpe_lit_fits_x86");
    emitter.instruction("mov BYTE PTR [r11 + r10], r8b");
    emitter.instruction("inc r10");
    emitter.instruction("inc rsi");
    emitter.instruction("jmp __rt_asf_qpe_consume1_x86");

    // -- SPACE or TAB: literal unless it is the run that ends a line --
    emitter.label("__rt_asf_qpe_ws_x86");
    emitter.instruction("test rbx, rbx");
    emitter.instruction("jz __rt_asf_qpe_ws_literal_x86");                                              // php only looks ahead when it knows the break
    emitter.instruction("lea rcx, [rsi + 1]");
    emitter.instruction("cmp rcx, rdi");
    emitter.instruction("jle __rt_asf_qpe_ws_scan_x86");
    // The budget is gone: php wraps FIRST, without consuming the byte, and re-decides.
    emitter.instruction("mov BYTE PTR [r11 + r10], 61");
    emitter.instruction("inc r10");
    emitter.instruction("xor rcx, rcx");
    emitter.label("__rt_asf_qpe_ws_bloop_x86");
    emitter.instruction("cmp rcx, r12");
    emitter.instruction("jge __rt_asf_qpe_ws_bend_x86");
    emitter.instruction("mov r8b, BYTE PTR [rbx + rcx]");
    emitter.instruction("mov BYTE PTR [r11 + r10], r8b");
    emitter.instruction("inc r10");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_asf_qpe_ws_bloop_x86");
    emitter.label("__rt_asf_qpe_ws_bend_x86");
    emitter.instruction("xor rsi, rsi");
    emitter.instruction("jmp __rt_asf_qpe_loop_x86");                                                   // re-decide on a fresh line
    emitter.label("__rt_asf_qpe_ws_scan_x86");
    // php scans from THIS byte to the last-but-one of the buffer.
    emitter.instruction("mov QWORD PTR [rsp + 8], r8");                                                 // the byte, needed again if the run is not trailing
    emitter.instruction("mov QWORD PTR [rsp + 16], r13");                                               // lb_cnt, which the scan borrows
    emitter.instruction("mov r15, 1");                                                                  // trail_ws counts this byte too
    emitter.instruction("xor r8, r8");                                                                  // lb_cnt2
    emitter.instruction("mov rcx, r9");                                                                 // ps2
    emitter.label("__rt_asf_qpe_ws_sloop_x86");
    emitter.instruction("lea r13, [rdx - 1]");
    emitter.instruction("cmp rcx, r13");
    emitter.instruction("jge __rt_asf_qpe_ws_sdone_x86");                                               // ran out: the run ends the buffer, so it is trailing
    emitter.instruction("mov r13b, BYTE PTR [rax + rcx]");
    emitter.instruction("cmp r13b, BYTE PTR [rbx + r8]");
    emitter.instruction("jne __rt_asf_qpe_ws_nomatch_x86");
    emitter.instruction("inc r8");
    emitter.instruction("cmp r8, r12");
    emitter.instruction("jge __rt_asf_qpe_ws_sdone_x86");                                               // the run does end a line
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_asf_qpe_ws_sloop_x86");
    emitter.label("__rt_asf_qpe_ws_nomatch_x86");
    emitter.instruction("test r8, r8");
    emitter.instruction("jnz __rt_asf_qpe_ws_notrail_x86");                                             // a partial break match broke off
    emitter.instruction("cmp r13b, 9");
    emitter.instruction("je __rt_asf_qpe_ws_more_x86");
    emitter.instruction("cmp r13b, 32");
    emitter.instruction("je __rt_asf_qpe_ws_more_x86");
    emitter.label("__rt_asf_qpe_ws_notrail_x86");
    emitter.instruction("xor r15, r15");                                                                // real content follows: nothing here is trailing
    emitter.instruction("jmp __rt_asf_qpe_ws_sdone_x86");
    emitter.label("__rt_asf_qpe_ws_more_x86");
    emitter.instruction("inc r15");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_asf_qpe_ws_sloop_x86");
    emitter.label("__rt_asf_qpe_ws_sdone_x86");
    emitter.instruction("mov r8, QWORD PTR [rsp + 8]");                                                 // restore the byte and lb_cnt the scan borrowed
    emitter.instruction("mov r13, QWORD PTR [rsp + 16]");
    emitter.instruction("test r15, r15");
    emitter.instruction("jnz __rt_asf_qpe_loop_x86");                                                   // trailing: go round and take the escape path
    emitter.label("__rt_asf_qpe_ws_literal_x86");
    emitter.instruction("mov BYTE PTR [r11 + r10], r8b");
    emitter.instruction("inc r10");
    emitter.instruction("inc rsi");
    emitter.instruction("jmp __rt_asf_qpe_consume1_x86");

    // -- anything else becomes `=XX`: three columns --
    emitter.label("__rt_asf_qpe_escape_x86");
    emitter.instruction("test rbx, rbx");
    emitter.instruction("jz __rt_asf_qpe_esc_fits_x86");
    emitter.instruction("lea rcx, [rsi + 3]");
    emitter.instruction("cmp rcx, rdi");
    emitter.instruction("jle __rt_asf_qpe_esc_fits_x86");
    emitter.instruction("mov BYTE PTR [r11 + r10], 61");
    emitter.instruction("inc r10");
    emitter.instruction("xor rcx, rcx");
    emitter.label("__rt_asf_qpe_esc_bloop_x86");
    emitter.instruction("cmp rcx, r12");
    emitter.instruction("jge __rt_asf_qpe_esc_bend_x86");
    emitter.instruction("mov QWORD PTR [rsp + 8], r8");                                                 // the break loop needs a byte register
    emitter.instruction("movzx r8d, BYTE PTR [rbx + rcx]");
    emitter.instruction("mov BYTE PTR [r11 + r10], r8b");
    emitter.instruction("mov r8, QWORD PTR [rsp + 8]");
    emitter.instruction("inc r10");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_asf_qpe_esc_bloop_x86");
    emitter.label("__rt_asf_qpe_esc_bend_x86");
    emitter.instruction("xor rsi, rsi");
    emitter.label("__rt_asf_qpe_esc_fits_x86");
    emitter.instruction("add rsi, 3");                                                                  // `=XX` occupies three columns
    emitter.instruction("mov BYTE PTR [r11 + r10], 61");
    emitter.instruction("inc r10");
    emitter.instruction("mov rcx, r8");
    emitter.instruction("shr rcx, 4");
    emitter.instruction("and rcx, 15");
    emitter.instruction("cmp rcx, 10");
    emitter.instruction("jl __rt_asf_qpe_hi_dig_x86");
    emitter.instruction("add rcx, 55");                                                                 // 10 -> 'A'
    emitter.instruction("jmp __rt_asf_qpe_hi_write_x86");
    emitter.label("__rt_asf_qpe_hi_dig_x86");
    emitter.instruction("add rcx, 48");                                                                 // 0 -> '0'
    emitter.label("__rt_asf_qpe_hi_write_x86");
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");
    emitter.instruction("inc r10");
    emitter.instruction("mov rcx, r8");
    emitter.instruction("and rcx, 15");
    emitter.instruction("cmp rcx, 10");
    emitter.instruction("jl __rt_asf_qpe_lo_dig_x86");
    emitter.instruction("add rcx, 55");
    emitter.instruction("jmp __rt_asf_qpe_lo_write_x86");
    emitter.label("__rt_asf_qpe_lo_dig_x86");
    emitter.instruction("add rcx, 48");
    emitter.label("__rt_asf_qpe_lo_write_x86");
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");
    emitter.instruction("inc r10");
    // php charges one byte of the line-ending run per escape it emits.
    emitter.instruction("test r15, r15");
    emitter.instruction("jz __rt_asf_qpe_consume1_x86");
    emitter.instruction("dec r15");

    // -- CONSUME_CHAR --
    emitter.label("__rt_asf_qpe_consume1_x86");
    emitter.instruction("cmp r14, r13");
    emitter.instruction("jge __rt_asf_qpe_consume_input_x86");
    emitter.instruction("inc r14");
    emitter.instruction("jmp __rt_asf_qpe_loop_x86");
    emitter.label("__rt_asf_qpe_consume_input_x86");
    emitter.instruction("xor r13, r13");
    emitter.instruction("xor r14, r14");
    emitter.instruction("inc r9");
    emitter.instruction("jmp __rt_asf_qpe_loop_x86");

    emitter.label("__rt_asf_qpe_copyback_x86");
    emitter.instruction("xor r9, r9");                                          // initialize the read cursor
    emitter.label("__rt_asf_qpe_cb_loop_x86");
    emitter.instruction("cmp r9, r10");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_qpe_done_x86");                           // finish the quoted-printable encoder operation when the bound is reached
    emitter.instruction("movzx ecx, BYTE PTR [r11 + r9]");                      // load the next byte from the scratch buffer
    emitter.instruction("mov BYTE PTR [rax + r9], cl");                         // copy scratch output back into the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("jmp __rt_asf_qpe_cb_loop_x86");                        // continue the quoted-printable encoder loop
    emitter.label("__rt_asf_qpe_done_x86");
    emitter.instruction("mov rdx, r10");                                        // return the transformed output length
    emitter.instruction("add rsp, 32");                                         // release the spill area
    emitter.instruction("pop r15");
    emitter.instruction("pop r14");
    emitter.instruction("pop r13");
    emitter.instruction("pop r12");
    emitter.instruction("pop rbx");
    emitter.instruction("ret");                                                 // return to the stream-filter caller
}
