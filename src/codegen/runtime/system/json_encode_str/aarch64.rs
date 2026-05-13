use crate::codegen::emit::Emitter;

/// ARM64 implementation of `__rt_json_encode_str`.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_str ---");
    emitter.label_global("__rt_json_encode_str");

    // -- set up stack frame --
    // Reserve 80 bytes (was 64) to stash x19 across the per-byte escape loop.
    // x19 is callee-saved per AAPCS64, so we save/restore it here and use it
    // to cache `_json_active_flags` for the whole encode call. Every flag
    // probe in the main loop then becomes a single `tst x19, #N` instead of
    // an addr-load + dereference.
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set new frame pointer
    emitter.instruction("str x19, [sp, #56]");                                  // save callee-saved x19 across the encode call

    // -- save inputs --
    emitter.instruction("str x1, [sp, #0]");                                    // save source ptr
    emitter.instruction("str x2, [sp, #8]");                                    // save source len

    // Cache _json_active_flags in x19 so the per-byte escape loop reads the
    // bitmask from a register instead of reloading from memory at every
    // HEX_*/UNESCAPED_*/UTF-8 dispatch site.
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x19, [x9]");                                       // x19 = cached active flag bitmask

    // -- JSON_NUMERIC_CHECK fast-path: numeric strings encode without quotes --
    emitter.instruction("tst x19, #32");                                        // is JSON_NUMERIC_CHECK (bit 32) set?
    emitter.instruction("b.eq __rt_json_str_quoted");                           // skip the numeric path when the flag is clear
    emitter.instruction("bl __rt_json_str_is_numeric");                         // does the input match the JSON number grammar?
    emitter.instruction("cbz x0, __rt_json_str_quoted");                        // non-numeric strings keep the quoted JSON form

    // -- numeric raw-copy path --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the source length
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // compute the write pointer
    emitter.instruction("str x11, [sp, #16]");                                  // save the output start pointer for the return slice
    emitter.instruction("mov x12, #0");                                         // initialize the copy index
    emitter.label("__rt_json_str_numeric_copy");
    emitter.instruction("cmp x12, x2");                                         // have we copied every byte?
    emitter.instruction("b.ge __rt_json_str_numeric_done");                     // exit when finished
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load the next source byte
    emitter.instruction("strb w13, [x11, x12]");                                // copy it directly to the concat buffer
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_json_str_numeric_copy");                        // continue copying
    emitter.label("__rt_json_str_numeric_done");
    emitter.instruction("add x10, x10, x2");                                    // advance the concat-buffer offset by the copied length
    emitter.instruction("str x10, [x9]");                                       // republish the concat-buffer offset
    emitter.instruction("ldr x1, [sp, #16]");                                   // x1 = output start (the copied slice)
    // x2 already holds the source length; reuse it as the result length.
    emitter.instruction("ldr x19, [sp, #56]");                                  // restore the callee-saved register
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate the scratch frame
    emitter.instruction("ret");                                                 // return the unquoted numeric slice

    emitter.label("__rt_json_str_quoted");

    // -- get output position in concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // output position
    emitter.instruction("str x11, [sp, #16]");                                  // save output start
    emitter.instruction("str x11, [sp, #24]");                                  // save output write pos

    // -- write opening quote --
    emitter.instruction("mov w12, #34");                                        // ASCII double quote
    emitter.instruction("strb w12, [x11]");                                     // write opening "
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos

    // -- loop through source string, escaping special chars --
    emitter.instruction("mov x13, #0");                                         // source index
    emitter.label("__rt_json_str_loop");
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload source len
    emitter.instruction("cmp x13, x2");                                         // check if done
    emitter.instruction("b.ge __rt_json_str_close");                            // done, write closing quote

    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source ptr
    emitter.instruction("ldrb w14, [x1, x13]");                                 // load source byte
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload write pos

    // -- check for special characters that need escaping --
    emitter.instruction("cmp w14, #34");                                        // check for double quote
    emitter.instruction("b.eq __rt_json_str_esc_quote");                        // escape it

    emitter.instruction("cmp w14, #92");                                        // check for backslash
    emitter.instruction("b.eq __rt_json_str_esc_backslash");                    // escape it

    emitter.instruction("cmp w14, #10");                                        // check for newline
    emitter.instruction("b.eq __rt_json_str_esc_n");                            // escape it

    emitter.instruction("cmp w14, #13");                                        // check for carriage return
    emitter.instruction("b.eq __rt_json_str_esc_r");                            // escape it

    emitter.instruction("cmp w14, #9");                                         // check for tab
    emitter.instruction("b.eq __rt_json_str_esc_t");                            // escape it

    emitter.instruction("cmp w14, #8");                                         // check for backspace
    emitter.instruction("b.eq __rt_json_str_esc_b");                            // escape it as \b
    emitter.instruction("cmp w14, #12");                                        // check for form-feed
    emitter.instruction("b.eq __rt_json_str_esc_f");                            // escape it as \f
    // Any remaining control byte (< 0x20) must be emitted as a \u00XX
    // escape so the encoder never produces invalid JSON. The \r/\n/\t/\b/\f
    // cases were filtered out above, so this catches 0x00..0x07, 0x0B, and
    // 0x0E..0x1F.
    emitter.instruction("cmp w14, #32");                                        // check for any remaining control byte (< 0x20)
    emitter.instruction("b.lt __rt_json_str_emit_ctrl_unicode");                // route through the unicode-escape helper

    // -- JSON_HEX_TAG: '<' and '>' optionally encoded as < / > --
    emitter.instruction("cmp w14, #60");                                        // check for '<'
    emitter.instruction("b.eq __rt_json_str_check_hex_tag");                    // route to the JSON_HEX_TAG flag check
    emitter.instruction("cmp w14, #62");                                        // check for '>'
    emitter.instruction("b.eq __rt_json_str_check_hex_tag");                    // route to the JSON_HEX_TAG flag check

    // -- JSON_HEX_AMP: '&' optionally encoded as & --
    emitter.instruction("cmp w14, #38");                                        // check for '&'
    emitter.instruction("b.eq __rt_json_str_check_hex_amp");                    // route to the JSON_HEX_AMP flag check

    // -- JSON_HEX_APOS: '\'' optionally encoded as ' --
    emitter.instruction("cmp w14, #39");                                        // check for '\''
    emitter.instruction("b.eq __rt_json_str_check_hex_apos");                   // route to the JSON_HEX_APOS flag check

    // -- forward slash: escape as \/ unless JSON_UNESCAPED_SLASHES is set --
    emitter.instruction("cmp w14, #47");                                        // check for forward slash
    emitter.instruction("b.ne __rt_json_str_check_unicode");                    // skip the slash branch when the byte is something else
    emitter.instruction("tst x19, #64");                                        // is JSON_UNESCAPED_SLASHES (bit 64) set? (cached flag)
    emitter.instruction("b.eq __rt_json_str_esc_slash");                        // when the flag is clear, escape the slash as \/
    emitter.instruction("b __rt_json_str_check_done");                          // flag set → copy the slash as-is

    // -- UTF-8 multibyte: escape to \uXXXX unless JSON_UNESCAPED_UNICODE is set --
    emitter.label("__rt_json_str_check_unicode");
    emitter.instruction("cmp w14, #128");                                       // is the source byte ASCII (< 0x80)?
    emitter.instruction("b.lo __rt_json_str_check_done");                       // ASCII bytes copy as-is
    // When JSON_INVALID_UTF8_IGNORE (0x100000) or JSON_INVALID_UTF8_SUBSTITUTE
    // (0x200000) is set, every multibyte byte must travel through the
    // validating dispatcher so malformed sequences are observed regardless
    // of JSON_UNESCAPED_UNICODE — the validator decides whether to skip,
    // substitute U+FFFD, or raise JSON_ERROR_UTF8.
    emitter.instruction("mov x10, #3145728");                                   // 0x300000 = JSON_INVALID_UTF8_IGNORE | JSON_INVALID_UTF8_SUBSTITUTE
    emitter.instruction("tst x19, x10");                                        // is either UTF-8 sanitization flag set? (cached flag)
    emitter.instruction("b.ne __rt_json_str_utf8_dispatch");                    // always validate when sanitization is requested
    emitter.instruction("tst x19, #256");                                       // is JSON_UNESCAPED_UNICODE (bit 256) set? (cached flag)
    emitter.instruction("b.ne __rt_json_str_check_done");                       // flag set → copy the multibyte sequence verbatim
    // Fall through to the UTF-8 decoder which emits the codepoint as
    // \uXXXX or \uHHHH\uLLLL (surrogate pair for codepoints >= 0x10000).
    emitter.instruction("b __rt_json_str_utf8_dispatch");                       // route to the UTF-8 length-class dispatcher

    emitter.label("__rt_json_str_check_done");

    // -- regular character, copy as-is --
    emitter.instruction("strb w14, [x11]");                                     // write char
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    emitter.label("__rt_json_str_esc_slash");
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write backslash
    emitter.instruction("mov w15, #47");                                        // forward slash
    emitter.instruction("strb w15, [x11, #1]");                                 // write the escaped slash byte
    emitter.instruction("add x11, x11, #2");                                    // advance past \/
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    // -- escape sequences --
    emitter.label("__rt_json_str_esc_quote");
    // Honor JSON_HEX_QUOT (bit 8): when set, embedded quotes become the
    // " sequence instead of the ordinary \" two-byte escape.
    emitter.instruction("tst x19, #8");                                         // is JSON_HEX_QUOT set? (cached flag)
    emitter.instruction("b.ne __rt_json_str_emit_hex");                         // route through the hex-escape helper when requested
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write backslash
    emitter.instruction("mov w15, #34");                                        // double quote
    emitter.instruction("strb w15, [x11, #1]");                                 // write escaped quote
    emitter.instruction("add x11, x11, #2");                                    // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    // -- JSON_HEX_TAG dispatch: hex-escape '<'/'>'  when bit 1 is set --
    emitter.label("__rt_json_str_check_hex_tag");
    emitter.instruction("tst x19, #1");                                         // is JSON_HEX_TAG set? (cached flag)
    emitter.instruction("b.ne __rt_json_str_emit_hex");                         // hex-escape the tag character when the flag is set
    emitter.instruction("b __rt_json_str_check_done");                          // otherwise copy the byte verbatim

    // -- JSON_HEX_AMP dispatch: hex-escape '&' when bit 2 is set --
    emitter.label("__rt_json_str_check_hex_amp");
    emitter.instruction("tst x19, #2");                                         // is JSON_HEX_AMP set? (cached flag)
    emitter.instruction("b.ne __rt_json_str_emit_hex");                         // hex-escape the ampersand when the flag is set
    emitter.instruction("b __rt_json_str_check_done");                          // otherwise copy the byte verbatim

    // -- JSON_HEX_APOS dispatch: hex-escape '\'' when bit 4 is set --
    emitter.label("__rt_json_str_check_hex_apos");
    emitter.instruction("tst x19, #4");                                         // is JSON_HEX_APOS set? (cached flag)
    emitter.instruction("b.ne __rt_json_str_emit_hex");                         // hex-escape the apostrophe when the flag is set
    emitter.instruction("b __rt_json_str_check_done");                          // otherwise copy the byte verbatim

    // -- shared hex-escape emission: writes the 6-byte \u00XX sequence --
    // Inputs: w14 = source byte, x11 = current write pointer, x13 = source index
    emitter.label("__rt_json_str_emit_hex");
    emitter.instruction("mov w15, #92");                                        // ASCII '\\'
    emitter.instruction("strb w15, [x11]");                                     // emit the backslash prefix
    emitter.instruction("mov w15, #117");                                       // ASCII 'u'
    emitter.instruction("strb w15, [x11, #1]");                                 // emit the unicode marker
    emitter.instruction("mov w15, #48");                                        // ASCII '0'
    emitter.instruction("strb w15, [x11, #2]");                                 // emit the high padding zero
    emitter.instruction("strb w15, [x11, #3]");                                 // emit the second high padding zero
    emitter.instruction("lsr w16, w14, #4");                                    // extract the high nibble
    emitter.instruction("and w16, w16, #0xF");                                  // mask to four bits
    emitter.instruction("cmp w16, #10");                                        // is the nibble in the 0..9 range?
    emitter.instruction("b.lt __rt_json_str_emit_hex_hi_dec");                  // decimal-digit branch for the high nibble
    emitter.instruction("add w16, w16, #7");                                    // shift A..F up to ASCII 'A'..'F'
    emitter.label("__rt_json_str_emit_hex_hi_dec");
    emitter.instruction("add w16, w16, #48");                                   // convert nibble to ASCII digit
    emitter.instruction("strb w16, [x11, #4]");                                 // emit the high hex digit
    emitter.instruction("and w16, w14, #0xF");                                  // extract the low nibble
    emitter.instruction("cmp w16, #10");                                        // is the nibble in the 0..9 range?
    emitter.instruction("b.lt __rt_json_str_emit_hex_lo_dec");                  // decimal-digit branch for the low nibble
    emitter.instruction("add w16, w16, #7");                                    // shift A..F up to ASCII 'A'..'F'
    emitter.label("__rt_json_str_emit_hex_lo_dec");
    emitter.instruction("add w16, w16, #48");                                   // convert nibble to ASCII digit
    emitter.instruction("strb w16, [x11, #5]");                                 // emit the low hex digit
    emitter.instruction("add x11, x11, #6");                                    // advance the write pointer past the 6-byte escape
    emitter.instruction("str x11, [sp, #24]");                                  // persist the updated write pointer
    emitter.instruction("add x13, x13, #1");                                    // advance to the next source byte
    emitter.instruction("b __rt_json_str_loop");                                // continue the main escape loop

    emitter.label("__rt_json_str_esc_backslash");
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write first backslash
    emitter.instruction("strb w15, [x11, #1]");                                 // write second backslash
    emitter.instruction("add x11, x11, #2");                                    // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    emitter.label("__rt_json_str_esc_n");
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write backslash
    emitter.instruction("mov w15, #110");                                       // 'n'
    emitter.instruction("strb w15, [x11, #1]");                                 // write 'n'
    emitter.instruction("add x11, x11, #2");                                    // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    emitter.label("__rt_json_str_esc_r");
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write backslash
    emitter.instruction("mov w15, #114");                                       // 'r'
    emitter.instruction("strb w15, [x11, #1]");                                 // write 'r'
    emitter.instruction("add x11, x11, #2");                                    // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    emitter.label("__rt_json_str_esc_t");
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write backslash
    emitter.instruction("mov w15, #116");                                       // 't'
    emitter.instruction("strb w15, [x11, #1]");                                 // write 't'
    emitter.instruction("add x11, x11, #2");                                    // advance by 2
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x13, x13, #1");                                    // next source char
    emitter.instruction("b __rt_json_str_loop");                                // continue

    emitter.label("__rt_json_str_esc_b");
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write backslash
    emitter.instruction("mov w15, #98");                                        // ASCII 'b'
    emitter.instruction("strb w15, [x11, #1]");                                 // write the backspace escape suffix
    emitter.instruction("add x11, x11, #2");                                    // advance the write pointer past the two-byte escape
    emitter.instruction("str x11, [sp, #24]");                                  // persist the updated write pointer
    emitter.instruction("add x13, x13, #1");                                    // advance to the next source byte
    emitter.instruction("b __rt_json_str_loop");                                // resume scanning the remaining source bytes

    emitter.label("__rt_json_str_esc_f");
    emitter.instruction("mov w15, #92");                                        // backslash
    emitter.instruction("strb w15, [x11]");                                     // write backslash
    emitter.instruction("mov w15, #102");                                       // ASCII 'f'
    emitter.instruction("strb w15, [x11, #1]");                                 // write the form-feed escape suffix
    emitter.instruction("add x11, x11, #2");                                    // advance the write pointer past the two-byte escape
    emitter.instruction("str x11, [sp, #24]");                                  // persist the updated write pointer
    emitter.instruction("add x13, x13, #1");                                    // advance to the next source byte
    emitter.instruction("b __rt_json_str_loop");                                // resume scanning the remaining source bytes

    // -- emit a generic control byte (< 0x20) as \u00XX --
    // Reuses the existing __rt_json_str_emit_u16 helper, which expects the
    // codepoint in w16 and the running write pointer in x11. We park x13
    // in scratch slot 40 across the helper call (the same slot used by the
    // UTF-8 BMP path).
    emitter.label("__rt_json_str_emit_ctrl_unicode");
    emitter.instruction("mov w16, w14");                                        // copy the control byte into the emit-helper input register
    emitter.instruction("str x13, [sp, #40]");                                  // checkpoint the source index across the helper call
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the running write pointer
    emitter.instruction("bl __rt_json_str_emit_u16");                           // emit \u00XX for the control byte
    emitter.instruction("ldr x13, [sp, #40]");                                  // restore the source index
    emitter.instruction("add x13, x13, #1");                                    // advance past the control byte
    emitter.instruction("b __rt_json_str_loop");                                // resume scanning the remaining source bytes

    // -- UTF-8 dispatcher: classify the lead byte and decode the codepoint --
    // Inputs: w14 = lead byte, x13 = source index, x1/x2 (saved at sp+0/sp+8)
    // Outputs: w16 = codepoint, x12 = bytes consumed (2/3/4)
    emitter.label("__rt_json_str_utf8_dispatch");
    // Validate the lead byte: 0x80..0xC1 are lone continuations or overlong
    // 2-byte starts; 0xF5+ are out of the Unicode range (or 5+-byte sequences
    // disallowed by RFC 3629). Both classes route to the malformed handler.
    emitter.instruction("cmp w14, #0xC2");                                      // is the lead byte below the smallest valid 2-byte start (0xC2)?
    emitter.instruction("b.lo __rt_json_str_utf8_malformed");                   // route to the malformed handler
    emitter.instruction("cmp w14, #0xF5");                                      // is the lead byte at or above the first invalid 4+ byte range (0xF5)?
    emitter.instruction("b.hs __rt_json_str_utf8_malformed");                   // route to the malformed handler

    emitter.instruction("cmp w14, #0xE0");                                      // lead byte 0xE0+ → 3- or 4-byte sequence
    emitter.instruction("b.lo __rt_json_str_utf8_2");                           // otherwise (0xC2..0xDF) → 2-byte sequence
    emitter.instruction("cmp w14, #0xF0");                                      // lead byte 0xF0+ → 4-byte sequence
    emitter.instruction("b.lo __rt_json_str_utf8_3");                           // otherwise (0xE0..0xEF) → 3-byte sequence
    emitter.instruction("b __rt_json_str_utf8_4");                              // 4-byte sequence (codepoint ≥ 0x10000)

    // -- 2-byte UTF-8 (codepoint 0x80..0x7FF) --
    emitter.label("__rt_json_str_utf8_2");
    // Bounds check the continuation byte before dereferencing the source
    // pointer. A truncated 2-byte sequence at the end of the input must
    // route to the malformed handler instead of reading past the buffer.
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the source length for the bounds check
    emitter.instruction("add x10, x13, #1");                                    // index of the continuation byte
    emitter.instruction("cmp x10, x2");                                         // is the continuation byte within the input?
    emitter.instruction("b.hs __rt_json_str_utf8_malformed");                   // truncated 2-byte sequence → malformed
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source pointer
    emitter.instruction("ldrb w15, [x1, x10]");                                 // load b2
    // Validate the continuation byte: every UTF-8 trailing byte must lie
    // in 0x80..0xBF; subtracting 0x80 and comparing against 0x40 separates
    // the valid range (0..0x3F) from everything else.
    emitter.instruction("sub w10, w15, #0x80");                                 // bias the byte so valid continuations land in 0..0x3F
    emitter.instruction("cmp w10, #0x40");                                      // is the byte outside the continuation range?
    emitter.instruction("b.hs __rt_json_str_utf8_malformed");                   // bad continuation → malformed
    emitter.instruction("and w16, w14, #0x1F");                                 // b1 & 0x1F → high 5 bits
    emitter.instruction("lsl w16, w16, #6");                                    // shift into bits 6..10 of the codepoint
    emitter.instruction("and w17, w15, #0x3F");                                 // b2 & 0x3F → low 6 bits
    emitter.instruction("orr w16, w16, w17");                                   // assemble the codepoint in w16
    emitter.instruction("mov x12, #2");                                         // 2 source bytes consumed
    emitter.instruction("b __rt_json_str_utf8_emit_bmp");                       // emit a single \uXXXX

    // -- 3-byte UTF-8 (codepoint 0x800..0xFFFF) --
    emitter.label("__rt_json_str_utf8_3");
    // Bounds check the final byte of the 3-byte sequence; if x13+2 is in
    // range, both x13+1 and x13+2 are valid offsets into the input buffer.
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the source length for the bounds check
    emitter.instruction("add x10, x13, #2");                                    // index of byte 3 (last byte of the sequence)
    emitter.instruction("cmp x10, x2");                                         // is the last byte within the input?
    emitter.instruction("b.hs __rt_json_str_utf8_malformed");                   // truncated 3-byte sequence → malformed
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source pointer
    emitter.instruction("add x10, x13, #1");                                    // index of byte 2
    emitter.instruction("ldrb w15, [x1, x10]");                                 // load b2
    // Validate b2 is a continuation byte (0x80..0xBF).
    emitter.instruction("sub w10, w15, #0x80");                                 // bias the byte so valid continuations land in 0..0x3F
    emitter.instruction("cmp w10, #0x40");                                      // is the byte outside the continuation range?
    emitter.instruction("b.hs __rt_json_str_utf8_malformed");                   // bad continuation → malformed
    emitter.instruction("add x10, x13, #2");                                    // index of byte 3
    emitter.instruction("ldrb w17, [x1, x10]");                                 // load b3
    // Validate b3 is a continuation byte.
    emitter.instruction("sub w10, w17, #0x80");                                 // bias the byte so valid continuations land in 0..0x3F
    emitter.instruction("cmp w10, #0x40");                                      // is the byte outside the continuation range?
    emitter.instruction("b.hs __rt_json_str_utf8_malformed");                   // bad continuation → malformed
    emitter.instruction("and w16, w14, #0x0F");                                 // b1 & 0x0F → high 4 bits
    emitter.instruction("lsl w16, w16, #12");                                   // shift into bits 12..15
    emitter.instruction("and w18, w15, #0x3F");                                 // b2 & 0x3F → middle 6 bits
    emitter.instruction("lsl w18, w18, #6");                                    // shift into bits 6..11
    emitter.instruction("orr w16, w16, w18");                                   // merge middle bits
    emitter.instruction("and w17, w17, #0x3F");                                 // b3 & 0x3F → low 6 bits
    emitter.instruction("orr w16, w16, w17");                                   // assemble the codepoint in w16
    emitter.instruction("mov x12, #3");                                         // 3 source bytes consumed
    emitter.instruction("b __rt_json_str_utf8_emit_bmp");                       // emit a single \uXXXX

    // -- 4-byte UTF-8 (codepoint 0x10000..0x10FFFF) → emit a surrogate pair --
    emitter.label("__rt_json_str_utf8_4");
    // Bounds check the final byte of the 4-byte sequence.
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the source length for the bounds check
    emitter.instruction("add x10, x13, #3");                                    // index of byte 4 (last byte of the sequence)
    emitter.instruction("cmp x10, x2");                                         // is the last byte within the input?
    emitter.instruction("b.hs __rt_json_str_utf8_malformed");                   // truncated 4-byte sequence → malformed
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source pointer
    emitter.instruction("add x10, x13, #1");                                    // index of byte 2
    emitter.instruction("ldrb w15, [x1, x10]");                                 // load b2
    // Validate b2 is a continuation byte.
    emitter.instruction("sub w10, w15, #0x80");                                 // bias the byte so valid continuations land in 0..0x3F
    emitter.instruction("cmp w10, #0x40");                                      // is the byte outside the continuation range?
    emitter.instruction("b.hs __rt_json_str_utf8_malformed");                   // bad continuation → malformed
    emitter.instruction("add x10, x13, #2");                                    // index of byte 3
    emitter.instruction("ldrb w17, [x1, x10]");                                 // load b3
    // Validate b3 is a continuation byte.
    emitter.instruction("sub w10, w17, #0x80");                                 // bias the byte so valid continuations land in 0..0x3F
    emitter.instruction("cmp w10, #0x40");                                      // is the byte outside the continuation range?
    emitter.instruction("b.hs __rt_json_str_utf8_malformed");                   // bad continuation → malformed
    emitter.instruction("add x10, x13, #3");                                    // index of byte 4
    emitter.instruction("ldrb w18, [x1, x10]");                                 // load b4
    // Validate b4 is a continuation byte.
    emitter.instruction("sub w10, w18, #0x80");                                 // bias the byte so valid continuations land in 0..0x3F
    emitter.instruction("cmp w10, #0x40");                                      // is the byte outside the continuation range?
    emitter.instruction("b.hs __rt_json_str_utf8_malformed");                   // bad continuation → malformed
    emitter.instruction("and w16, w14, #0x07");                                 // b1 & 0x07 → top 3 bits
    emitter.instruction("lsl w16, w16, #18");                                   // shift into bits 18..20
    emitter.instruction("and w19, w15, #0x3F");                                 // b2 & 0x3F → 6 bits
    emitter.instruction("lsl w19, w19, #12");                                   // shift into bits 12..17
    emitter.instruction("orr w16, w16, w19");                                   // merge bits 12..17
    emitter.instruction("and w19, w17, #0x3F");                                 // b3 & 0x3F → 6 bits
    emitter.instruction("lsl w19, w19, #6");                                    // shift into bits 6..11
    emitter.instruction("orr w16, w16, w19");                                   // merge bits 6..11
    emitter.instruction("and w18, w18, #0x3F");                                 // b4 & 0x3F → low 6 bits
    emitter.instruction("orr w16, w16, w18");                                   // full 21-bit codepoint
    // Compute the surrogate pair: cp -= 0x10000; high = 0xD800 + (cp >> 10);
    // low = 0xDC00 + (cp & 0x3FF).
    emitter.instruction("mov w20, #0x10000");                                   // 0x10000 base offset for surrogate pair maths
    emitter.instruction("sub w16, w16, w20");                                   // cp -= 0x10000
    emitter.instruction("lsr w19, w16, #10");                                   // (cp >> 10) → high surrogate index
    emitter.instruction("mov w20, #0xD800");                                    // base of the high-surrogate range
    emitter.instruction("add w19, w19, w20");                                   // high surrogate codepoint in w19
    emitter.instruction("and w20, w16, #0x3FF");                                // (cp & 0x3FF) → low surrogate index
    emitter.instruction("mov w21, #0xDC00");                                    // base of the low-surrogate range
    emitter.instruction("add w20, w20, w21");                                   // low surrogate codepoint in w20

    // Emit \uHHHH (high surrogate) — load write pos, run the emitter helper.
    emitter.instruction("mov w16, w19");                                        // copy the high surrogate into the emitter input register
    emitter.instruction("str x13, [sp, #40]");                                  // checkpoint the source index in scratch slot 40 (sp+16 is the output start)
    emitter.instruction("str x20, [sp, #32]");                                  // park the low-surrogate codepoint across the helper call (slot is otherwise free mid-loop)
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the running write pointer
    emitter.instruction("bl __rt_json_str_emit_u16");                           // emit \uHHHH for the high surrogate
    emitter.instruction("ldr x20, [sp, #32]");                                  // restore the low-surrogate codepoint
    emitter.instruction("mov w16, w20");                                        // pass the low surrogate to the emitter
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the running write pointer (now past the high surrogate)
    emitter.instruction("bl __rt_json_str_emit_u16");                           // emit \uLLLL for the low surrogate
    emitter.instruction("ldr x13, [sp, #40]");                                  // restore the source index from scratch slot 40
    emitter.instruction("add x13, x13, #4");                                    // advance the source index past the 4-byte UTF-8 sequence
    emitter.instruction("b __rt_json_str_loop");                                // continue scanning the remaining source bytes

    // -- emit a BMP codepoint as a single \uXXXX --
    emitter.label("__rt_json_str_utf8_emit_bmp");
    emitter.instruction("str x13, [sp, #40]");                                  // checkpoint the source index in scratch slot 40
    emitter.instruction("str x12, [sp, #32]");                                  // checkpoint the bytes-consumed count across the helper call
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the running write pointer
    emitter.instruction("bl __rt_json_str_emit_u16");                           // emit \uXXXX
    emitter.instruction("ldr x12, [sp, #32]");                                  // restore the bytes-consumed count
    emitter.instruction("ldr x13, [sp, #40]");                                  // restore the source index
    emitter.instruction("add x13, x13, x12");                                   // advance past the consumed source bytes
    emitter.instruction("b __rt_json_str_loop");                                // continue scanning the remaining source bytes

    // -- malformed UTF-8 handler --
    // Reached from any UTF-8 dispatch site that detected an invalid lead
    // byte, a truncated multi-byte sequence, or a non-continuation trailing
    // byte. The handler observes the active flag bitmask and chooses one
    // of three recoveries:
    //   * JSON_INVALID_UTF8_IGNORE     (bit 0x100000) — drop the byte and
    //     keep encoding;
    //   * JSON_INVALID_UTF8_SUBSTITUTE (bit 0x200000) — emit U+FFFD and
    //     keep encoding;
    //   * neither flag — record JSON_ERROR_UTF8 (5) through the throw helper
    //     so JsonException is raised when JSON_THROW_ON_ERROR is set, and
    //     fall through to the IGNORE recovery on the no-throw path so the
    //     encoder still produces partial output.
    emitter.label("__rt_json_str_utf8_malformed");
    emitter.instruction("str x13, [sp, #40]");                                  // park the source index across the helper-call sequence
    emitter.instruction("mov x10, #1048576");                                   // JSON_INVALID_UTF8_IGNORE = bit 0x100000
    emitter.instruction("tst x19, x10");                                        // is the IGNORE flag set? (cached flag)
    emitter.instruction("b.ne __rt_json_str_utf8_skip_byte");                   // skip the byte and keep encoding
    emitter.instruction("mov x10, #2097152");                                   // JSON_INVALID_UTF8_SUBSTITUTE = bit 0x200000
    emitter.instruction("tst x19, x10");                                        // is the SUBSTITUTE flag set? (cached flag)
    emitter.instruction("b.ne __rt_json_str_utf8_substitute_byte");             // emit U+FFFD then skip the byte
    // Neither sanitization flag is set: report JSON_ERROR_UTF8 and let the
    // throw helper raise when JSON_THROW_ON_ERROR is requested. When it
    // returns (no-throw path), continue with the IGNORE recovery so the
    // encoder still emits well-formed JSON for the rest of the input.
    emitter.instruction("mov x0, #5");                                          // JSON_ERROR_UTF8
    emitter.instruction("bl __rt_json_throw_error");                            // record the error and throw when JSON_THROW_ON_ERROR is set
    emitter.instruction("b __rt_json_str_utf8_skip_byte");                      // fall through to skip the malformed byte

    // -- malformed UTF-8: emit U+FFFD then advance one byte --
    emitter.label("__rt_json_str_utf8_substitute_byte");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the running write pointer
    emitter.instruction("mov w16, #0xFFFD");                                    // U+FFFD REPLACEMENT CHARACTER
    emitter.instruction("bl __rt_json_str_emit_u16");                           // emit � into the concat buffer
    // Fall through to skip the malformed byte after substituting.

    // -- malformed UTF-8: skip a single byte and resume the main loop --
    emitter.label("__rt_json_str_utf8_skip_byte");
    emitter.instruction("ldr x13, [sp, #40]");                                  // restore the source index parked above
    emitter.instruction("add x13, x13, #1");                                    // advance past exactly one malformed byte (PHP-faithful recovery)
    emitter.instruction("b __rt_json_str_loop");                                // resume scanning the remaining source bytes

    // -- helper: classify a string as a JSON number per RFC 8259 --
    // Inputs: x1 = source ptr, x2 = source length
    // Output: x0 = 1 when the entire input matches the JSON number grammar,
    //         0 otherwise. Clobbers w9, w10, w11.
    emitter.label("__rt_json_str_is_numeric");
    emitter.instruction("mov x0, #0");                                          // default to non-numeric
    emitter.instruction("cbz x2, __rt_json_str_is_numeric_done");               // empty string is not numeric
    emitter.instruction("mov x9, #0");                                          // initialize the source index

    // Optional leading minus sign.
    emitter.instruction("ldrb w10, [x1, x9]");                                  // peek the first byte
    emitter.instruction("cmp w10, #45");                                        // is it '-'?
    emitter.instruction("b.ne __rt_json_str_is_numeric_int_start");             // no minus → start integer part
    emitter.instruction("add x9, x9, #1");                                      // consume the minus sign
    emitter.instruction("cmp x9, x2");                                          // is there anything after the minus?
    emitter.instruction("b.ge __rt_json_str_is_numeric_done");                  // bare '-' is not numeric

    // First digit of the integer part is mandatory.
    emitter.label("__rt_json_str_is_numeric_int_start");
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the next byte
    emitter.instruction("sub w11, w10, #48");                                   // ASCII digit → 0..9 if valid
    emitter.instruction("cmp w11, #9");                                         // is it in 0..9?
    emitter.instruction("b.hi __rt_json_str_is_numeric_done");                  // first byte must be a digit
    emitter.instruction("add x9, x9, #1");                                      // advance past the first digit

    // Remaining integer digits.
    emitter.label("__rt_json_str_is_numeric_int_loop");
    emitter.instruction("cmp x9, x2");                                          // end of input?
    emitter.instruction("b.ge __rt_json_str_is_numeric_ok");                    // pure integer → numeric
    emitter.instruction("ldrb w10, [x1, x9]");                                  // peek the next byte
    emitter.instruction("sub w11, w10, #48");                                   // is it a digit?
    emitter.instruction("cmp w11, #9");                                         // 0..9 range
    emitter.instruction("b.hi __rt_json_str_is_numeric_after_int");             // non-digit → check fraction or exponent
    emitter.instruction("add x9, x9, #1");                                      // consume the digit
    emitter.instruction("b __rt_json_str_is_numeric_int_loop");                 // continue scanning digits

    emitter.label("__rt_json_str_is_numeric_after_int");
    emitter.instruction("cmp w10, #46");                                        // is it '.'?
    emitter.instruction("b.eq __rt_json_str_is_numeric_frac_start");            // start fraction
    emitter.instruction("cmp w10, #101");                                       // is it 'e'?
    emitter.instruction("b.eq __rt_json_str_is_numeric_exp_sign");              // start exponent
    emitter.instruction("cmp w10, #69");                                        // is it 'E'?
    emitter.instruction("b.eq __rt_json_str_is_numeric_exp_sign");              // start exponent
    emitter.instruction("b __rt_json_str_is_numeric_done");                     // any other byte → not numeric

    // Fractional part: at least one digit required after the decimal point.
    emitter.label("__rt_json_str_is_numeric_frac_start");
    emitter.instruction("add x9, x9, #1");                                      // consume the '.'
    emitter.instruction("cmp x9, x2");                                          // is there a digit after?
    emitter.instruction("b.ge __rt_json_str_is_numeric_done");                  // bare 'X.' is not numeric
    emitter.instruction("ldrb w10, [x1, x9]");                                  // peek
    emitter.instruction("sub w11, w10, #48");                                   // is it a digit?
    emitter.instruction("cmp w11, #9");                                         // 0..9 range
    emitter.instruction("b.hi __rt_json_str_is_numeric_done");                  // need at least one fractional digit
    emitter.instruction("add x9, x9, #1");                                      // consume the first fractional digit

    emitter.label("__rt_json_str_is_numeric_frac_loop");
    emitter.instruction("cmp x9, x2");                                          // end of input?
    emitter.instruction("b.ge __rt_json_str_is_numeric_ok");                    // fraction-only number → numeric
    emitter.instruction("ldrb w10, [x1, x9]");                                  // peek
    emitter.instruction("sub w11, w10, #48");                                   // digit?
    emitter.instruction("cmp w11, #9");                                         // 0..9
    emitter.instruction("b.hi __rt_json_str_is_numeric_after_frac");            // non-digit → check exponent
    emitter.instruction("add x9, x9, #1");                                      // consume the digit
    emitter.instruction("b __rt_json_str_is_numeric_frac_loop");                // continue

    emitter.label("__rt_json_str_is_numeric_after_frac");
    emitter.instruction("cmp w10, #101");                                       // 'e'?
    emitter.instruction("b.eq __rt_json_str_is_numeric_exp_sign");              // start exponent
    emitter.instruction("cmp w10, #69");                                        // 'E'?
    emitter.instruction("b.eq __rt_json_str_is_numeric_exp_sign");              // start exponent
    emitter.instruction("b __rt_json_str_is_numeric_done");                     // anything else → not numeric

    // Exponent: optional sign, then at least one digit.
    emitter.label("__rt_json_str_is_numeric_exp_sign");
    emitter.instruction("add x9, x9, #1");                                      // consume the 'e' or 'E'
    emitter.instruction("cmp x9, x2");                                          // is there anything after?
    emitter.instruction("b.ge __rt_json_str_is_numeric_done");                  // bare 'Xe' is not numeric
    emitter.instruction("ldrb w10, [x1, x9]");                                  // peek
    emitter.instruction("cmp w10, #43");                                        // optional '+'?
    emitter.instruction("b.eq __rt_json_str_is_numeric_exp_advance_sign");      // branch on the current JSON string encoder condition
    emitter.instruction("cmp w10, #45");                                        // optional '-'?
    emitter.instruction("b.eq __rt_json_str_is_numeric_exp_advance_sign");      // branch on the current JSON string encoder condition
    emitter.instruction("b __rt_json_str_is_numeric_exp_first_digit");          // continue in the JSON string encoder control path
    emitter.label("__rt_json_str_is_numeric_exp_advance_sign");
    emitter.instruction("add x9, x9, #1");                                      // consume the exponent sign
    emitter.instruction("cmp x9, x2");                                          // is there a digit after?
    emitter.instruction("b.ge __rt_json_str_is_numeric_done");                  // bare 'e+' / 'e-' is not numeric

    emitter.label("__rt_json_str_is_numeric_exp_first_digit");
    emitter.instruction("ldrb w10, [x1, x9]");                                  // peek the first exponent digit
    emitter.instruction("sub w11, w10, #48");                                   // digit?
    emitter.instruction("cmp w11, #9");                                         // 0..9
    emitter.instruction("b.hi __rt_json_str_is_numeric_done");                  // need at least one exponent digit
    emitter.instruction("add x9, x9, #1");                                      // consume the digit

    emitter.label("__rt_json_str_is_numeric_exp_loop");
    emitter.instruction("cmp x9, x2");                                          // end of input?
    emitter.instruction("b.ge __rt_json_str_is_numeric_ok");                    // valid exponent reached EOI → numeric
    emitter.instruction("ldrb w10, [x1, x9]");                                  // peek
    emitter.instruction("sub w11, w10, #48");                                   // digit?
    emitter.instruction("cmp w11, #9");                                         // check the current JSON string encoder condition
    emitter.instruction("b.hi __rt_json_str_is_numeric_done");                  // any non-digit after exponent digits → not numeric
    emitter.instruction("add x9, x9, #1");                                      // consume the digit
    emitter.instruction("b __rt_json_str_is_numeric_exp_loop");                 // continue

    emitter.label("__rt_json_str_is_numeric_ok");
    emitter.instruction("mov x0, #1");                                          // signal numeric
    emitter.label("__rt_json_str_is_numeric_done");
    emitter.instruction("ret");                                                 // return result in x0

    // -- helper: write \uXXXX to [x11], advance x11 by 6, persist write pos --
    emitter.label("__rt_json_str_emit_u16");
    emitter.instruction("mov w15, #92");                                        // ASCII '\\'
    emitter.instruction("strb w15, [x11]");                                     // emit the backslash prefix
    emitter.instruction("mov w15, #117");                                       // ASCII 'u'
    emitter.instruction("strb w15, [x11, #1]");                                 // emit the unicode marker
    // Nibble 3 (bits 12-15)
    emitter.instruction("lsr w17, w16, #12");                                   // extract bits 12..15
    emitter.instruction("and w17, w17, #0xF");                                  // mask to four bits
    emitter.instruction("cmp w17, #10");                                        // is the nibble in 0..9?
    emitter.instruction("b.lt __rt_json_str_emit_u16_n3_dec");                  // decimal-digit branch
    emitter.instruction("add w17, w17, #7");                                    // shift A..F up to ASCII range
    emitter.label("__rt_json_str_emit_u16_n3_dec");
    emitter.instruction("add w17, w17, #48");                                   // convert nibble to ASCII digit
    emitter.instruction("strb w17, [x11, #2]");                                 // emit the digit
    // Nibble 2 (bits 8-11)
    emitter.instruction("lsr w17, w16, #8");                                    // extract bits 8..11
    emitter.instruction("and w17, w17, #0xF");                                  // mask to four bits
    emitter.instruction("cmp w17, #10");                                        // is the nibble in 0..9?
    emitter.instruction("b.lt __rt_json_str_emit_u16_n2_dec");                  // decimal-digit branch
    emitter.instruction("add w17, w17, #7");                                    // shift A..F up to ASCII range
    emitter.label("__rt_json_str_emit_u16_n2_dec");
    emitter.instruction("add w17, w17, #48");                                   // convert nibble to ASCII digit
    emitter.instruction("strb w17, [x11, #3]");                                 // emit the digit
    // Nibble 1 (bits 4-7)
    emitter.instruction("lsr w17, w16, #4");                                    // extract bits 4..7
    emitter.instruction("and w17, w17, #0xF");                                  // mask to four bits
    emitter.instruction("cmp w17, #10");                                        // is the nibble in 0..9?
    emitter.instruction("b.lt __rt_json_str_emit_u16_n1_dec");                  // decimal-digit branch
    emitter.instruction("add w17, w17, #7");                                    // shift A..F up to ASCII range
    emitter.label("__rt_json_str_emit_u16_n1_dec");
    emitter.instruction("add w17, w17, #48");                                   // convert nibble to ASCII digit
    emitter.instruction("strb w17, [x11, #4]");                                 // emit the digit
    // Nibble 0 (bits 0-3)
    emitter.instruction("and w17, w16, #0xF");                                  // extract low nibble
    emitter.instruction("cmp w17, #10");                                        // is the nibble in 0..9?
    emitter.instruction("b.lt __rt_json_str_emit_u16_n0_dec");                  // decimal-digit branch
    emitter.instruction("add w17, w17, #7");                                    // shift A..F up to ASCII range
    emitter.label("__rt_json_str_emit_u16_n0_dec");
    emitter.instruction("add w17, w17, #48");                                   // convert nibble to ASCII digit
    emitter.instruction("strb w17, [x11, #5]");                                 // emit the digit
    emitter.instruction("add x11, x11, #6");                                    // advance the write pointer past the escape
    emitter.instruction("str x11, [sp, #24]");                                  // persist the updated write pointer
    emitter.instruction("ret");                                                 // return to the caller

    // -- write closing quote --
    emitter.label("__rt_json_str_close");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload write pos
    emitter.instruction("mov w12, #34");                                        // ASCII double quote
    emitter.instruction("strb w12, [x11]");                                     // write closing "
    emitter.instruction("add x11, x11, #1");                                    // advance past closing quote

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #16]");                                   // x1 = output start
    emitter.instruction("sub x2, x11, x1");                                     // x2 = total length

    // -- update concat_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x2");                                    // add result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- tear down and return --
    emitter.instruction("ldr x19, [sp, #56]");                                  // restore the callee-saved register
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
