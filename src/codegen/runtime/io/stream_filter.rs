//! Purpose:
//! Emits the `__rt_apply_stream_filter` runtime helper, which applies a
//! built-in stream filter to a buffer in place.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//! - `__rt_fread` (read direction) and the `fwrite` emitter (write direction).
//!
//! Key details:
//! - Filter ids: 1 = `string.toupper`, 2 = `string.tolower`, 3 = `string.rot13`.
//!   All three are 1:1 byte transforms, so the buffer length never changes.
//! - This is a leaf helper: it transforms the buffer in place and preserves the
//!   pointer/length registers so callers can return them unchanged.

use crate::codegen::{emit::Emitter, platform::Arch};
use crate::codegen::abi;

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
    emitter.instruction("cmp x3, #4");                                          // filter id 4 = string.strip_tags
    emitter.instruction("b.eq __rt_asf_strip_tags");                            // dispatch to the strip-tags state machine
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

    // -- string.strip_tags: state-machine compaction. Output ≤ input;
    //    returns the compacted length in x0 so fread/fwrite can use it. --
    emitter.label("__rt_asf_strip_tags");
    emitter.instruction("mov x5, #0");                                          // read index
    emitter.instruction("mov x6, #0");                                          // write index
    emitter.instruction("mov x7, #0");                                          // in_tag flag (0 = outside tag, 1 = inside)
    emitter.label("__rt_asf_strip_loop");
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_strip_done");                            // finish the strip-tags operation when the bound is reached
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("cbnz x7, __rt_asf_strip_in_tag");                      // use the active-state path when the flag is set
    // not in tag: '<' enters tag; everything else is written.
    emitter.instruction("cmp w8, #60");                                         // test for '<' to enter an HTML tag
    emitter.instruction("b.eq __rt_asf_strip_enter");                           // dispatch to __rt_asf_strip_enter when the comparison matches
    emitter.instruction("strb w8, [x1, x6]");                                   // write byte
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("b __rt_asf_strip_advance");                            // continue at __rt_asf_strip_advance
    emitter.label("__rt_asf_strip_enter");
    emitter.instruction("mov x7, #1");                                          // initialize the filter accumulator or state flag
    emitter.instruction("b __rt_asf_strip_advance");                            // continue at __rt_asf_strip_advance
    emitter.label("__rt_asf_strip_in_tag");
    // inside tag: '>' exits; otherwise skip the byte.
    emitter.instruction("cmp w8, #62");                                         // test for '>' to leave an HTML tag
    emitter.instruction("b.ne __rt_asf_strip_advance");                         // continue scanning when the delimiter was not found
    emitter.instruction("mov x7, #0");                                          // initialize the filter accumulator or state flag
    emitter.label("__rt_asf_strip_advance");
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("b __rt_asf_strip_loop");                               // continue the strip-tags loop
    emitter.label("__rt_asf_strip_done");
    emitter.instruction("mov x2, x6");                                          // return compacted length via the same register fread/fwrite use for length
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

    // -- dechunk: parse HTTP/1.1 chunked transfer-encoding inline.
    //    Format: <hex_size>\r\n<bytes>\r\n<hex_size>\r\n<bytes>\r\n...0\r\n\r\n
    //    Output is the concatenation of all <bytes> chunks, with the
    //    size-lines and CRLFs removed. In-place compaction (output ≤
    //    input) using read/write cursors. --
    emitter.label("__rt_asf_dechunk");
    emitter.instruction("mov x5, #0");                                          // read index
    emitter.instruction("mov x6, #0");                                          // write index
    emitter.label("__rt_asf_dechunk_size_loop");
    // Parse a hex chunk-size line: accumulate hex digits in x7 until \r\n.
    emitter.instruction("mov x7, #0");                                          // chunk size accumulator
    emitter.label("__rt_asf_dechunk_size_read");
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_done");                          // finish the dechunk operation when the bound is reached
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("cmp w8, #13");                                         // test for carriage return in the encoded stream
    emitter.instruction("b.eq __rt_asf_dechunk_size_eol");                      // end of size line
    emitter.instruction("cmp w8, #59");                                         // ';' (chunk extensions)
    emitter.instruction("b.eq __rt_asf_dechunk_skip_to_eol");                   // ignore extensions
    // Hex digit?
    emitter.instruction("cmp w8, #48");                                         // test for the lower bound of ASCII digits
    emitter.instruction("b.lt __rt_asf_dechunk_size_read");                     // skip non-digit
    emitter.instruction("cmp w8, #57");                                         // test for the upper bound of ASCII digits
    emitter.instruction("b.le __rt_asf_dechunk_size_digit");                    // accept values inside the current range
    // letter? case-fold via |0x20.
    emitter.instruction("orr w8, w8, #0x20");                                   // merge the extracted bits into the accumulator
    emitter.instruction("cmp w8, #97");                                         // test for the lower bound of lowercase hex/base64 letters
    emitter.instruction("b.lt __rt_asf_dechunk_size_read");                     // reject values below the accepted range
    emitter.instruction("cmp w8, #102");                                        // test for line feed in the encoded stream
    emitter.instruction("b.gt __rt_asf_dechunk_size_read");                     // reject values above the accepted range
    emitter.instruction("sub w8, w8, #87");                                     // a..f → 10..15 (97-87)
    emitter.instruction("b __rt_asf_dechunk_size_acc");                         // continue at __rt_asf_dechunk_size_acc
    emitter.label("__rt_asf_dechunk_size_digit");
    emitter.instruction("sub w8, w8, #48");                                     // 0..9 → 0..9
    emitter.label("__rt_asf_dechunk_size_acc");
    emitter.instruction("lsl x7, x7, #4");                                      // shift the accumulator to make room for the next value
    emitter.instruction("orr x7, x7, x8");                                      // merge the extracted bits into the accumulator
    emitter.instruction("b __rt_asf_dechunk_size_read");                        // continue at __rt_asf_dechunk_size_read
    emitter.label("__rt_asf_dechunk_skip_to_eol");
    // Skip everything until \r.
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_done");                          // finish the dechunk operation when the bound is reached
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("cmp w8, #13");                                         // test for carriage return in the encoded stream
    emitter.instruction("b.ne __rt_asf_dechunk_skip_to_eol");                   // continue at __rt_asf_dechunk_skip_to_eol when the comparison does not match
    emitter.label("__rt_asf_dechunk_size_eol");
    // Expect '\n' (LF) after the \r. Skip it if present.
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_done");                          // finish the dechunk operation when the bound is reached
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("cmp w8, #10");                                         // test for line feed in the encoded stream
    emitter.instruction("b.ne __rt_asf_dechunk_skip_lf");                       // continue at __rt_asf_dechunk_skip_lf when the comparison does not match
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.label("__rt_asf_dechunk_skip_lf");
    // chunk size 0 → end.
    emitter.instruction("cbz x7, __rt_asf_dechunk_done");                       // finish the dechunk operation when the count is zero
    // Copy x7 bytes from [x1+x5] to [x1+x6].
    emitter.instruction("mov x9, #0");                                          // set up the next value for this filter step
    emitter.label("__rt_asf_dechunk_copy_loop");
    emitter.instruction("cmp x9, x7");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_copy_done");                     // finish the dechunk operation when the bound is reached
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_done");                          // finish the dechunk operation when the bound is reached
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("strb w8, [x1, x6]");                                   // write output back into the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("add x9, x9, #1");                                      // advance the chunk-copy cursor
    emitter.instruction("b __rt_asf_dechunk_copy_loop");                        // continue the dechunk loop
    emitter.label("__rt_asf_dechunk_copy_done");
    // Skip trailing \r\n after chunk data.
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_size_loop");                     // leave the loop when the cursor reaches its bound
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("cmp w8, #13");                                         // test for carriage return in the encoded stream
    emitter.instruction("b.ne __rt_asf_dechunk_size_loop");                     // continue the dechunk loop when the delimiter was not found
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_dechunk_size_loop");                     // leave the loop when the cursor reaches its bound
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("cmp w8, #10");                                         // test for line feed in the encoded stream
    emitter.instruction("b.ne __rt_asf_dechunk_size_loop");                     // continue the dechunk loop when the delimiter was not found
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("b __rt_asf_dechunk_size_loop");                        // continue the dechunk loop
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
    crate::codegen::abi::emit_symbol_address(emitter, "x4", "_stream_grow_scratch");
    crate::codegen::abi::emit_symbol_address(emitter, "x15", "_b64_encode_tbl");
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
    // memcpy scratch[0..x6] back into x1.
    emitter.instruction("mov x5, #0");                                          // initialize the read cursor
    emitter.label("__rt_asf_b64e_cb_loop");
    emitter.instruction("cmp x5, x6");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_b64e_done");                             // finish the base64 encoder operation when the bound is reached
    emitter.instruction("ldrb w11, [x4, x5]");                                  // load the next byte from the scratch buffer
    emitter.instruction("strb w11, [x1, x5]");                                  // copy scratch output back into the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("b __rt_asf_b64e_cb_loop");                             // continue the base64 encoder loop
    emitter.label("__rt_asf_b64e_done");
    emitter.instruction("mov x2, x6");                                          // return encoded length
    emitter.instruction("ret");                                                 // return to the stream-filter caller

    // -- convert.quoted-printable-encode: bytes outside 33..126 (and '=' itself)
    //    become '=XX' hex escapes. Encodes into _stream_grow_scratch and memcpy
    //    back. Caps input at 21845 bytes (worst case = 3x growth) so output
    //    fits 65536. --
    emitter.label("__rt_asf_qp_encode");
    emitter.instruction("mov x4, #21845");                                      // ~64KB/3 worst-case cap
    emitter.instruction("cmp x2, x4");                                          // check whether the current cursor reached its bound
    emitter.instruction("csel x2, x4, x2, gt");                                 // x2 = MIN(x2, 21845)
    crate::codegen::abi::emit_symbol_address(emitter, "x4", "_stream_grow_scratch");
    crate::codegen::abi::emit_symbol_address(emitter, "x15", "_b64_encode_tbl");
    // hex table is just '0'..'9','A'..'F' so build inline instead.
    emitter.instruction("mov x5, #0");                                          // read index
    emitter.instruction("mov x6, #0");                                          // write index
    emitter.label("__rt_asf_qpe_loop");
    emitter.instruction("cmp x5, x2");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_qpe_copyback");                          // copy back once input encoding is complete
    emitter.instruction("ldrb w8, [x1, x5]");                                   // load the next byte from the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    // Pass-through printable ASCII (33..60, 62..126) directly.
    emitter.instruction("cmp w8, #33");                                         // check whether a full three-byte group is available
    emitter.instruction("b.lt __rt_asf_qpe_escape");                            // reject values below the accepted range
    emitter.instruction("cmp w8, #126");                                        // check whether only one byte remains
    emitter.instruction("b.gt __rt_asf_qpe_escape");                            // reject values above the accepted range
    emitter.instruction("cmp w8, #61");                                         // test for '=' before quoted-printable escaping
    emitter.instruction("b.eq __rt_asf_qpe_escape");                            // escape the current byte
    emitter.instruction("strb w8, [x4, x6]");                                   // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("b __rt_asf_qpe_loop");                                 // continue the quoted-printable encoder loop
    emitter.label("__rt_asf_qpe_escape");
    // Emit '=' then two hex digits.
    emitter.instruction("mov w9, #61");                                         // '='
    emitter.instruction("strb w9, [x4, x6]");                                   // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    // High nibble.
    emitter.instruction("lsr w9, w8, #4");                                      // extract the next sextet or byte from the accumulated bits
    emitter.instruction("and w9, w9, #0xF");                                    // mask the bits needed for the next encoded byte
    emitter.instruction("cmp w9, #10");                                         // test for line feed in the encoded stream
    emitter.instruction("b.lt __rt_asf_qpe_hi_dig");                            // reject values below the accepted range
    emitter.instruction("add w9, w9, #55");                                     // 10 → 'A' (10+55=65)
    emitter.instruction("b __rt_asf_qpe_hi_write");                             // continue at __rt_asf_qpe_hi_write
    emitter.label("__rt_asf_qpe_hi_dig");
    emitter.instruction("add w9, w9, #48");                                     // 0 → '0'
    emitter.label("__rt_asf_qpe_hi_write");
    emitter.instruction("strb w9, [x4, x6]");                                   // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    // Low nibble.
    emitter.instruction("and w9, w8, #0xF");                                    // mask the bits needed for the next encoded byte
    emitter.instruction("cmp w9, #10");                                         // test for line feed in the encoded stream
    emitter.instruction("b.lt __rt_asf_qpe_lo_dig");                            // reject values below the accepted range
    emitter.instruction("add w9, w9, #55");                                     // convert the nibble value into an ASCII hex digit
    emitter.instruction("b __rt_asf_qpe_lo_write");                             // continue at __rt_asf_qpe_lo_write
    emitter.label("__rt_asf_qpe_lo_dig");
    emitter.instruction("add w9, w9, #48");                                     // convert the nibble value into an ASCII hex digit
    emitter.label("__rt_asf_qpe_lo_write");
    emitter.instruction("strb w9, [x4, x6]");                                   // write encoded output into the scratch buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the write cursor
    emitter.instruction("b __rt_asf_qpe_loop");                                 // continue the quoted-printable encoder loop
    emitter.label("__rt_asf_qpe_copyback");
    emitter.instruction("mov x5, #0");                                          // initialize the read cursor
    emitter.label("__rt_asf_qpe_cb_loop");
    emitter.instruction("cmp x5, x6");                                          // check whether the current cursor reached its bound
    emitter.instruction("b.ge __rt_asf_qpe_done");                              // finish the quoted-printable encoder operation when the bound is reached
    emitter.instruction("ldrb w11, [x4, x5]");                                  // load the next byte from the scratch buffer
    emitter.instruction("strb w11, [x1, x5]");                                  // copy scratch output back into the stream buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the read cursor
    emitter.instruction("b __rt_asf_qpe_cb_loop");                              // continue the quoted-printable encoder loop
    emitter.label("__rt_asf_qpe_done");
    emitter.instruction("mov x2, x6");                                          // return encoded length
    emitter.instruction("ret");                                                 // return to the stream-filter caller
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
    emitter.instruction("cmp rcx, 4");                                          // filter id 4 = string.strip_tags
    emitter.instruction("je __rt_asf_strip_tags_x86");                          // dispatch to the strip-tags state machine
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

    // -- string.strip_tags: state-machine compaction. --
    emitter.label("__rt_asf_strip_tags_x86");
    emitter.instruction("xor r9, r9");                                          // read index
    emitter.instruction("xor r10, r10");                                        // write index
    emitter.instruction("xor r11, r11");                                        // in_tag flag
    emitter.label("__rt_asf_strip_loop_x86");
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_strip_done_x86");                         // finish the strip-tags operation when the bound is reached
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("test r11, r11");                                       // check whether the current counter or flag is zero
    emitter.instruction("jnz __rt_asf_strip_in_tag_x86");                       // use the active-state path when the flag is set
    emitter.instruction("cmp r8b, 60");                                         // test for '<' to enter an HTML tag
    emitter.instruction("je __rt_asf_strip_enter_x86");                         // dispatch to __rt_asf_strip_enter_x86 when the comparison matches
    emitter.instruction("mov BYTE PTR [rax + r10], r8b");                       // write output back into the stream buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("jmp __rt_asf_strip_advance_x86");                      // continue at __rt_asf_strip_advance_x86
    emitter.label("__rt_asf_strip_enter_x86");
    emitter.instruction("mov r11, 1");                                          // initialize the filter accumulator or state flag
    emitter.instruction("jmp __rt_asf_strip_advance_x86");                      // continue at __rt_asf_strip_advance_x86
    emitter.label("__rt_asf_strip_in_tag_x86");
    emitter.instruction("cmp r8b, 62");                                         // test for '>' to leave an HTML tag
    emitter.instruction("jne __rt_asf_strip_advance_x86");                      // continue scanning when the delimiter was not found
    emitter.instruction("xor r11, r11");                                        // initialize the filter accumulator or state flag
    emitter.label("__rt_asf_strip_advance_x86");
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("jmp __rt_asf_strip_loop_x86");                         // continue the strip-tags loop
    emitter.label("__rt_asf_strip_done_x86");
    emitter.instruction("mov rdx, r10");                                        // return compacted length via the same register fread/fwrite use for length
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

    // -- dechunk (x86_64) — HTTP/1.1 chunked transfer-encoding parser --
    emitter.label("__rt_asf_dechunk_x86");
    emitter.instruction("xor r9, r9");                                          // read index
    emitter.instruction("xor r10, r10");                                        // write index
    emitter.label("__rt_asf_dc_size_loop_x86");
    emitter.instruction("xor r11, r11");                                        // chunk size accumulator
    emitter.label("__rt_asf_dc_size_read_x86");
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_done_x86");                            // finish the dechunk operation when the bound is reached
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("cmp r8b, 13");                                         // test for carriage return in the encoded stream
    emitter.instruction("je __rt_asf_dc_size_eol_x86");                         // finish parsing the current line
    emitter.instruction("cmp r8b, 59");                                         // ';' ext
    emitter.instruction("je __rt_asf_dc_skip_eol_x86");                         // finish parsing the current line
    emitter.instruction("cmp r8b, 48");                                         // test for the lower bound of ASCII digits
    emitter.instruction("jl __rt_asf_dc_size_read_x86");                        // reject values below the accepted range
    emitter.instruction("cmp r8b, 57");                                         // test for the upper bound of ASCII digits
    emitter.instruction("jle __rt_asf_dc_size_digit_x86");                      // accept values inside the current range
    emitter.instruction("or r8b, 0x20");                                        // case-fold to lower
    emitter.instruction("cmp r8b, 97");                                         // test for the lower bound of lowercase hex/base64 letters
    emitter.instruction("jl __rt_asf_dc_size_read_x86");                        // reject values below the accepted range
    emitter.instruction("cmp r8b, 102");                                        // test for line feed in the encoded stream
    emitter.instruction("jg __rt_asf_dc_size_read_x86");                        // reject values above the accepted range
    emitter.instruction("sub r8b, 87");                                         // a..f → 10..15
    emitter.instruction("jmp __rt_asf_dc_size_acc_x86");                        // continue at __rt_asf_dc_size_acc_x86
    emitter.label("__rt_asf_dc_size_digit_x86");
    emitter.instruction("sub r8b, 48");                                         // convert an ASCII digit into its numeric value
    emitter.label("__rt_asf_dc_size_acc_x86");
    emitter.instruction("shl r11, 4");                                          // shift the accumulator to make room for the next value
    emitter.instruction("movzx r8, r8b");                                       // zero-extend the parsed chunk-size digit
    emitter.instruction("or r11, r8");                                          // merge the extracted bits into the accumulator
    emitter.instruction("jmp __rt_asf_dc_size_read_x86");                       // continue at __rt_asf_dc_size_read_x86
    emitter.label("__rt_asf_dc_skip_eol_x86");
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_done_x86");                            // finish the dechunk operation when the bound is reached
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("cmp r8b, 13");                                         // test for carriage return in the encoded stream
    emitter.instruction("jne __rt_asf_dc_skip_eol_x86");                        // continue at __rt_asf_dc_skip_eol_x86 when the comparison does not match
    emitter.label("__rt_asf_dc_size_eol_x86");
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_done_x86");                            // finish the dechunk operation when the bound is reached
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("cmp r8b, 10");                                         // test for line feed in the encoded stream
    emitter.instruction("jne __rt_asf_dc_skip_lf_x86");                         // continue at __rt_asf_dc_skip_lf_x86 when the comparison does not match
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.label("__rt_asf_dc_skip_lf_x86");
    emitter.instruction("test r11, r11");                                       // check whether the current counter or flag is zero
    emitter.instruction("jz __rt_asf_dc_done_x86");                             // finish the dechunk operation when the count is zero
    // Copy r11 bytes.
    emitter.instruction("xor r12, r12");                                        // initialize the base64 group count
    emitter.label("__rt_asf_dc_copy_loop_x86");
    emitter.instruction("cmp r12, r11");                                        // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_copy_done_x86");                       // finish the dechunk operation when the bound is reached
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_done_x86");                            // finish the dechunk operation when the bound is reached
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("mov BYTE PTR [rax + r10], r8b");                       // write output back into the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("inc r12");                                             // advance the chunk-copy cursor
    emitter.instruction("jmp __rt_asf_dc_copy_loop_x86");                       // continue the dechunk loop
    emitter.label("__rt_asf_dc_copy_done_x86");
    // Skip trailing \r\n.
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_size_loop_x86");                       // leave the loop when the cursor reaches its bound
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("cmp r8b, 13");                                         // test for carriage return in the encoded stream
    emitter.instruction("jne __rt_asf_dc_size_loop_x86");                       // continue the dechunk loop when the delimiter was not found
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_dc_size_loop_x86");                       // leave the loop when the cursor reaches its bound
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("cmp r8b, 10");                                         // test for line feed in the encoded stream
    emitter.instruction("jne __rt_asf_dc_size_loop_x86");                       // continue the dechunk loop when the delimiter was not found
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("jmp __rt_asf_dc_size_loop_x86");                       // continue the dechunk loop
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
    emitter.label("__rt_asf_b64e_copyback_x86");
    emitter.instruction("xor r9, r9");                                          // initialize the read cursor
    emitter.label("__rt_asf_b64e_cb_loop_x86");
    emitter.instruction("cmp r9, r10");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_b64e_done_x86");                          // finish the base64 encoder operation when the bound is reached
    emitter.instruction("movzx ecx, BYTE PTR [r11 + r9]");                      // load the next byte from the scratch buffer
    emitter.instruction("mov BYTE PTR [rax + r9], cl");                         // copy scratch output back into the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("jmp __rt_asf_b64e_cb_loop_x86");                       // continue the base64 encoder loop
    emitter.label("__rt_asf_b64e_done_x86");
    emitter.instruction("mov rdx, r10");                                        // return the transformed output length
    emitter.instruction("ret");                                                 // return to the stream-filter caller

    // -- convert.quoted-printable-encode (x86_64) --
    emitter.label("__rt_asf_qp_encode_x86");
    emitter.instruction("mov r11, 21845");                                      // set the maximum input length that fits the scratch buffer
    emitter.instruction("cmp rdx, r11");                                        // check whether the current cursor reached its bound
    emitter.instruction("cmovg rdx, r11");                                      // rdx = MIN(rdx, 21845)
    abi::emit_symbol_address(emitter, "r11", "_stream_grow_scratch");           // load the scratch buffer base address
    emitter.instruction("xor r9, r9");                                          // initialize the read cursor
    emitter.instruction("xor r10, r10");                                        // initialize the write cursor
    emitter.label("__rt_asf_qpe_loop_x86");
    emitter.instruction("cmp r9, rdx");                                         // check whether the current cursor reached its bound
    emitter.instruction("jge __rt_asf_qpe_copyback_x86");                       // copy back once input encoding is complete
    emitter.instruction("movzx r8d, BYTE PTR [rax + r9]");                      // load the next byte from the stream buffer
    emitter.instruction("inc r9");                                              // advance the read cursor
    emitter.instruction("cmp r8b, 33");                                         // check whether a full three-byte group is available
    emitter.instruction("jl __rt_asf_qpe_escape_x86");                          // reject values below the accepted range
    emitter.instruction("cmp r8b, 126");                                        // check whether only one byte remains
    emitter.instruction("jg __rt_asf_qpe_escape_x86");                          // reject values above the accepted range
    emitter.instruction("cmp r8b, 61");                                         // test for '=' before quoted-printable escaping
    emitter.instruction("je __rt_asf_qpe_escape_x86");                          // escape the current byte
    emitter.instruction("mov BYTE PTR [r11 + r10], r8b");                       // write encoded output into the scratch buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("jmp __rt_asf_qpe_loop_x86");                           // continue the quoted-printable encoder loop
    emitter.label("__rt_asf_qpe_escape_x86");
    emitter.instruction("mov BYTE PTR [r11 + r10], 61");                        // '='
    emitter.instruction("inc r10");                                             // advance the write cursor
    // hi nibble
    emitter.instruction("mov rcx, r8");                                         // stage bits before extracting the next output byte
    emitter.instruction("shr rcx, 4");                                          // extract the next sextet or byte from the accumulated bits
    emitter.instruction("and rcx, 15");                                         // mask the bits needed for the next encoded byte
    emitter.instruction("cmp rcx, 10");                                         // test for line feed in the encoded stream
    emitter.instruction("jl __rt_asf_qpe_hi_dig_x86");                          // reject values below the accepted range
    emitter.instruction("add rcx, 55");                                         // convert the nibble value into an ASCII hex digit
    emitter.instruction("jmp __rt_asf_qpe_hi_write_x86");                       // continue at __rt_asf_qpe_hi_write_x86
    emitter.label("__rt_asf_qpe_hi_dig_x86");
    emitter.instruction("add rcx, 48");                                         // convert the nibble value into an ASCII hex digit
    emitter.label("__rt_asf_qpe_hi_write_x86");
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // write encoded output into the scratch buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    // lo nibble
    emitter.instruction("mov rcx, r8");                                         // stage bits before extracting the next output byte
    emitter.instruction("and rcx, 15");                                         // mask the bits needed for the next encoded byte
    emitter.instruction("cmp rcx, 10");                                         // test for line feed in the encoded stream
    emitter.instruction("jl __rt_asf_qpe_lo_dig_x86");                          // reject values below the accepted range
    emitter.instruction("add rcx, 55");                                         // convert the nibble value into an ASCII hex digit
    emitter.instruction("jmp __rt_asf_qpe_lo_write_x86");                       // continue at __rt_asf_qpe_lo_write_x86
    emitter.label("__rt_asf_qpe_lo_dig_x86");
    emitter.instruction("add rcx, 48");                                         // convert the nibble value into an ASCII hex digit
    emitter.label("__rt_asf_qpe_lo_write_x86");
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // write encoded output into the scratch buffer
    emitter.instruction("inc r10");                                             // advance the write cursor
    emitter.instruction("jmp __rt_asf_qpe_loop_x86");                           // continue the quoted-printable encoder loop
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
    emitter.instruction("ret");                                                 // return to the stream-filter caller
}
