use crate::codegen::emit::Emitter;

/// x86_64 implementation of `__rt_json_encode_str`.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_str ---");
    emitter.label_global("__rt_json_encode_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving JSON-string scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source slice and concat-buffer cursors
    // Frame bumped from 64 to 80 bytes to stash r15 across the per-byte
    // escape loop. r15 is callee-saved (System V), so we save/restore it
    // here and use it to cache `_json_active_flags` for the whole encode
    // call. Every flag probe in the main loop then becomes a single
    // `test r15, N` instead of an rip-relative load.
    emitter.instruction("sub rsp, 80");                                         // reserve local slots + slot for the cached-flag callee-saved register
    emitter.instruction("mov QWORD PTR [rbp - 80], r15");                       // save callee-saved r15 across the encode call
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source string pointer across the JSON escaping loop
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source string length across the JSON escaping loop

    // Cache _json_active_flags in r15 so the per-byte escape loop reads
    // the bitmask from a register instead of reloading from memory at
    // every HEX_*/UNESCAPED_*/UTF-8 dispatch site.
    emitter.instruction("mov r15, QWORD PTR [rip + _json_active_flags]");       // r15 = cached active flag bitmask

    // -- JSON_NUMERIC_CHECK fast-path: numeric strings encode without quotes --
    emitter.instruction("test r15, 32");                                        // is JSON_NUMERIC_CHECK (bit 32) set? (cached flag)
    emitter.instruction("je __rt_json_str_quoted_x");                           // skip the numeric path when the flag is clear
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source pointer for the numeric helper
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the source length for the numeric helper
    emitter.instruction("call __rt_json_str_is_numeric_x");                     // does the input match the JSON number grammar?
    emitter.instruction("test rax, rax");                                       // helper returned 1 for numeric, 0 otherwise
    emitter.instruction("je __rt_json_str_quoted_x");                           // non-numeric strings keep the quoted JSON form

    // -- numeric raw-copy path --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the source length
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer offset
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base
    emitter.instruction("add r11, r10");                                        // compute the write pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the output start pointer for the return slice
    emitter.instruction("xor rcx, rcx");                                        // initialize the copy index
    emitter.label("__rt_json_str_numeric_copy_x");
    emitter.instruction("cmp rcx, rdx");                                        // have we copied every byte?
    emitter.instruction("jae __rt_json_str_numeric_done_x");                    // exit when finished
    emitter.instruction("mov r9b, BYTE PTR [rax + rcx]");                       // load the next source byte
    emitter.instruction("mov BYTE PTR [r11 + rcx], r9b");                       // copy directly to the concat buffer
    emitter.instruction("add rcx, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_json_str_numeric_copy_x");                    // continue copying
    emitter.label("__rt_json_str_numeric_done_x");
    emitter.instruction("add r10, rdx");                                        // advance the concat-buffer offset by the copied length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r10");              // republish the concat-buffer offset
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // rax = output start (the copied slice)
    // rdx already holds the source length; reuse it as the result length.
    emitter.instruction("mov r15, QWORD PTR [rbp - 80]");                       // restore the callee-saved register
    emitter.instruction("mov rsp, rbp");                                        // unwind the scratch frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the unquoted numeric slice

    emitter.label("__rt_json_str_quoted_x");
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer absolute offset before appending the JSON string
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the current JSON append
    emitter.instruction("add r11, r10");                                        // compute the current concat-buffer write pointer from the base plus offset
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the encoded-string start pointer for the final result slice
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the current concat-buffer write pointer for the escape loop
    emitter.instruction("mov BYTE PTR [r11], 34");                              // write the opening JSON quote before any escaped payload bytes
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the opening quote
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer before entering the source-byte loop
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the source-byte index to the beginning of the input string

    emitter.label("__rt_json_str_loop");
    emitter.instruction("mov r13, QWORD PTR [rbp - 40]");                       // reload the current source-byte index at the top of the JSON escape loop
    emitter.instruction("cmp r13, QWORD PTR [rbp - 16]");                       // have we consumed every byte of the source string?
    emitter.instruction("jae __rt_json_str_close");                             // finish by writing the closing quote once the whole source slice has been escaped
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source string pointer for the current byte fetch
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the current concat-buffer write pointer before appending the next escaped byte
    emitter.instruction("movzx r14, BYTE PTR [r10 + r13]");                     // load the next source byte and widen it so escape comparisons stay unsigned
    emitter.instruction("cmp r14b, 34");                                        // does the source byte equal a JSON double quote?
    emitter.instruction("je __rt_json_str_esc_quote");                          // escape embedded double quotes as \\"
    emitter.instruction("cmp r14b, 92");                                        // does the source byte equal a backslash?
    emitter.instruction("je __rt_json_str_esc_backslash");                      // escape embedded backslashes as \\\\
    emitter.instruction("cmp r14b, 10");                                        // does the source byte equal a newline?
    emitter.instruction("je __rt_json_str_esc_n");                              // escape newlines as \\n
    emitter.instruction("cmp r14b, 13");                                        // does the source byte equal a carriage return?
    emitter.instruction("je __rt_json_str_esc_r");                              // escape carriage returns as \\r
    emitter.instruction("cmp r14b, 9");                                         // does the source byte equal a horizontal tab?
    emitter.instruction("je __rt_json_str_esc_t");                              // escape tabs as \\t

    emitter.instruction("cmp r14b, 8");                                         // does the source byte equal a backspace?
    emitter.instruction("je __rt_json_str_esc_b");                              // escape it as \\b
    emitter.instruction("cmp r14b, 12");                                        // does the source byte equal a form-feed?
    emitter.instruction("je __rt_json_str_esc_f");                              // escape it as \\f
    // Any remaining control byte (< 0x20) routes through the unicode-escape
    // helper so the encoder never produces invalid JSON. The \\r/\\n/\\t/\\b/\\f
    // cases were filtered out above, so this catches 0x00..0x07, 0x0B, and
    // 0x0E..0x1F.
    emitter.instruction("cmp r14b, 32");                                        // is the source byte a remaining control byte (< 0x20)?
    emitter.instruction("jb __rt_json_str_emit_ctrl_unicode");                  // route through the unicode-escape helper

    // -- JSON_HEX_TAG: '<' and '>' optionally encoded as \\u003C / \\u003E --
    emitter.instruction("cmp r14b, 60");                                        // does the source byte equal '<'?
    emitter.instruction("je __rt_json_str_check_hex_tag_x");                    // route to the JSON_HEX_TAG flag check
    emitter.instruction("cmp r14b, 62");                                        // does the source byte equal '>'?
    emitter.instruction("je __rt_json_str_check_hex_tag_x");                    // route to the JSON_HEX_TAG flag check
    // -- JSON_HEX_AMP: '&' optionally encoded as \\u0026 --
    emitter.instruction("cmp r14b, 38");                                        // does the source byte equal '&'?
    emitter.instruction("je __rt_json_str_check_hex_amp_x");                    // route to the JSON_HEX_AMP flag check
    // -- JSON_HEX_APOS: '\\'' optionally encoded as \\u0027 --
    emitter.instruction("cmp r14b, 39");                                        // does the source byte equal '\\''?
    emitter.instruction("je __rt_json_str_check_hex_apos_x");                   // route to the JSON_HEX_APOS flag check

    emitter.instruction("cmp r14b, 47");                                        // does the source byte equal a forward slash?
    emitter.instruction("jne __rt_json_str_check_unicode_x");                   // skip the slash branch when the byte is something else
    emitter.instruction("test r15, 64");                                        // is JSON_UNESCAPED_SLASHES (bit 64) set? (cached flag)
    emitter.instruction("je __rt_json_str_esc_slash");                          // when the flag is clear, escape the slash as \/
    emitter.instruction("jmp __rt_json_str_check_done");                        // flag set → copy the slash as-is

    // -- UTF-8 multibyte: escape to \uXXXX unless JSON_UNESCAPED_UNICODE is set --
    emitter.label("__rt_json_str_check_unicode_x");
    emitter.instruction("cmp r14b, 128");                                       // is the source byte ASCII (< 0x80)?
    emitter.instruction("jb __rt_json_str_check_done");                         // ASCII bytes copy as-is
    // JSON_INVALID_UTF8_IGNORE (0x100000) and JSON_INVALID_UTF8_SUBSTITUTE
    // (0x200000) require validating every multibyte byte through the UTF-8
    // dispatcher, even when JSON_UNESCAPED_UNICODE is set, so malformed
    // sequences are observed instead of being copied verbatim.
    emitter.instruction("test r15, 0x300000");                                  // any UTF-8 sanitization flag set? (cached flag)
    emitter.instruction("jne __rt_json_str_utf8_dispatch_x");                   // always validate when sanitization is requested
    emitter.instruction("test r15, 256");                                       // is JSON_UNESCAPED_UNICODE (bit 256) set? (cached flag)
    emitter.instruction("jne __rt_json_str_check_done");                        // flag set → copy the multibyte sequence verbatim
    emitter.instruction("jmp __rt_json_str_utf8_dispatch_x");                   // route to the UTF-8 length-class dispatcher

    emitter.label("__rt_json_str_check_done");
    emitter.instruction("mov BYTE PTR [r11], r14b");                            // copy ordinary bytes directly into the concat buffer without any escape expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer after the copied ordinary byte
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after copying the ordinary byte
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after copying the ordinary byte
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_slash");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the escape backslash that prefixes a JSON slash escape
    emitter.instruction("mov BYTE PTR [r11 + 1], 47");                          // write the escaped slash byte
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the two-byte slash escape
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the slash escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the slash
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_quote");
    // Honor JSON_HEX_QUOT (bit 8): when set, embedded quotes become the
    // \\u0022 sequence instead of the ordinary \\" two-byte escape.
    emitter.instruction("test r15, 8");                                         // is JSON_HEX_QUOT set? (cached flag)
    emitter.instruction("jne __rt_json_str_emit_hex_x");                        // route through the hex-escape helper when requested
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the escape backslash that prefixes an embedded JSON quote
    emitter.instruction("mov BYTE PTR [r11 + 1], 34");                          // write the escaped JSON quote after the backslash prefix
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the two-byte escape sequence
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the quote escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the embedded quote
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    // -- JSON_HEX_TAG dispatch: hex-escape '<'/'>' when bit 1 is set --
    emitter.label("__rt_json_str_check_hex_tag_x");
    emitter.instruction("test r15, 1");                                         // is JSON_HEX_TAG set? (cached flag)
    emitter.instruction("jne __rt_json_str_emit_hex_x");                        // hex-escape the tag character when the flag is set
    emitter.instruction("jmp __rt_json_str_check_done");                        // otherwise copy the byte verbatim

    // -- JSON_HEX_AMP dispatch: hex-escape '&' when bit 2 is set --
    emitter.label("__rt_json_str_check_hex_amp_x");
    emitter.instruction("test r15, 2");                                         // is JSON_HEX_AMP set? (cached flag)
    emitter.instruction("jne __rt_json_str_emit_hex_x");                        // hex-escape the ampersand when the flag is set
    emitter.instruction("jmp __rt_json_str_check_done");                        // otherwise copy the byte verbatim

    // -- JSON_HEX_APOS dispatch: hex-escape '\\'' when bit 4 is set --
    emitter.label("__rt_json_str_check_hex_apos_x");
    emitter.instruction("test r15, 4");                                         // is JSON_HEX_APOS set? (cached flag)
    emitter.instruction("jne __rt_json_str_emit_hex_x");                        // hex-escape the apostrophe when the flag is set
    emitter.instruction("jmp __rt_json_str_check_done");                        // otherwise copy the byte verbatim

    // -- shared hex-escape emission: writes the 6-byte \\u00XX sequence --
    // Inputs: r14b = source byte, r11 = current write pointer, r13 = source index
    emitter.label("__rt_json_str_emit_hex_x");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // emit the backslash prefix
    emitter.instruction("mov BYTE PTR [r11 + 1], 117");                         // emit the unicode marker 'u'
    emitter.instruction("mov BYTE PTR [r11 + 2], 48");                          // emit the high padding zero
    emitter.instruction("mov BYTE PTR [r11 + 3], 48");                          // emit the second high padding zero
    emitter.instruction("movzx r9, r14b");                                      // widen the source byte for arithmetic on a full register
    emitter.instruction("mov r12, r9");                                         // copy the byte for the high-nibble extraction
    emitter.instruction("shr r12, 4");                                          // extract the high nibble
    emitter.instruction("and r12, 0xF");                                        // mask to four bits
    emitter.instruction("cmp r12, 10");                                         // is the nibble in the 0..9 range?
    emitter.instruction("jl __rt_json_str_emit_hex_hi_dec_x");                  // decimal-digit branch for the high nibble
    emitter.instruction("add r12, 7");                                          // shift A..F up to ASCII 'A'..'F'
    emitter.label("__rt_json_str_emit_hex_hi_dec_x");
    emitter.instruction("add r12, 48");                                         // convert nibble to ASCII digit
    emitter.instruction("mov BYTE PTR [r11 + 4], r12b");                        // emit the high hex digit
    emitter.instruction("mov r12, r9");                                         // copy the byte again for the low-nibble extraction
    emitter.instruction("and r12, 0xF");                                        // extract the low nibble
    emitter.instruction("cmp r12, 10");                                         // is the nibble in the 0..9 range?
    emitter.instruction("jl __rt_json_str_emit_hex_lo_dec_x");                  // decimal-digit branch for the low nibble
    emitter.instruction("add r12, 7");                                          // shift A..F up to ASCII 'A'..'F'
    emitter.label("__rt_json_str_emit_hex_lo_dec_x");
    emitter.instruction("add r12, 48");                                         // convert nibble to ASCII digit
    emitter.instruction("mov BYTE PTR [r11 + 5], r12b");                        // emit the low hex digit
    emitter.instruction("add r11, 6");                                          // advance the write pointer past the 6-byte escape
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer
    emitter.instruction("add r13, 1");                                          // advance to the next source byte
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_backslash");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the first backslash of the escaped backslash pair
    emitter.instruction("mov BYTE PTR [r11 + 1], 92");                          // write the second backslash of the escaped backslash pair
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the escaped backslash pair
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the backslash escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the backslash
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_n");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the escape backslash that prefixes a JSON newline escape
    emitter.instruction("mov BYTE PTR [r11 + 1], 110");                         // write the JSON newline escape codepoint n
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the two-byte newline escape
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the newline escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the newline
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_r");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the escape backslash that prefixes a JSON carriage-return escape
    emitter.instruction("mov BYTE PTR [r11 + 1], 114");                         // write the JSON carriage-return escape codepoint r
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the two-byte carriage-return escape
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the carriage-return escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the carriage return
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_t");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the escape backslash that prefixes a JSON tab escape
    emitter.instruction("mov BYTE PTR [r11 + 1], 116");                         // write the JSON tab escape codepoint t
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the two-byte tab escape
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the tab escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the tab
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_b");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the escape backslash that prefixes a JSON backspace escape
    emitter.instruction("mov BYTE PTR [r11 + 1], 98");                          // write the JSON backspace escape codepoint b
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the two-byte backspace escape
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the backspace escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the backspace
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    emitter.label("__rt_json_str_esc_f");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // write the escape backslash that prefixes a JSON form-feed escape
    emitter.instruction("mov BYTE PTR [r11 + 1], 102");                         // write the JSON form-feed escape codepoint f
    emitter.instruction("add r11, 2");                                          // advance the concat-buffer write pointer past the two-byte form-feed escape
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer after expanding the form-feed escape
    emitter.instruction("add r13, 1");                                          // advance to the next source byte after consuming the form-feed
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // continue escaping the remaining source bytes

    // -- emit a generic control byte (< 0x20) as \\u00XX --
    // Reuses the existing __rt_json_str_emit_u16_x helper, which expects
    // the codepoint in rdi and the running write pointer in r11.
    emitter.label("__rt_json_str_emit_ctrl_unicode");
    emitter.instruction("movzx rdi, r14b");                                     // pass the control-byte codepoint to the emit helper
    emitter.instruction("mov QWORD PTR [rbp - 48], r13");                       // checkpoint the source index across the helper call
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the running write pointer
    emitter.instruction("call __rt_json_str_emit_u16_x");                       // emit \\u00XX for the control byte
    emitter.instruction("mov r13, QWORD PTR [rbp - 48]");                       // restore the source index
    emitter.instruction("add r13, 1");                                          // advance past the control byte
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source-byte index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // resume scanning the remaining source bytes

    // -- UTF-8 dispatcher: classify the lead byte and decode the codepoint --
    emitter.label("__rt_json_str_utf8_dispatch_x");
    // Validate the lead byte: 0x80..0xC1 are lone continuation bytes or
    // overlong 2-byte starts; 0xF5+ are above the Unicode range (or 5+-byte
    // sequences forbidden by RFC 3629). Both classes route to the
    // malformed handler so JSON_INVALID_UTF8_* and JSON_ERROR_UTF8 see
    // the same input each implementation observes on ARM64.
    emitter.instruction("cmp r14b, 0xC2");                                      // is the lead byte below the smallest valid 2-byte start (0xC2)?
    emitter.instruction("jb __rt_json_str_utf8_malformed_x");                   // route to the malformed handler
    emitter.instruction("cmp r14b, 0xF5");                                      // is the lead byte at/above the first invalid 4+ byte range (0xF5)?
    emitter.instruction("jae __rt_json_str_utf8_malformed_x");                  // route to the malformed handler

    emitter.instruction("cmp r14b, 0xE0");                                      // lead byte 0xE0+ → 3- or 4-byte sequence
    emitter.instruction("jb __rt_json_str_utf8_2_x");                           // otherwise (0xC2..0xDF) → 2-byte sequence
    emitter.instruction("cmp r14b, 0xF0");                                      // lead byte 0xF0+ → 4-byte sequence
    emitter.instruction("jb __rt_json_str_utf8_3_x");                           // otherwise (0xE0..0xEF) → 3-byte sequence
    emitter.instruction("jmp __rt_json_str_utf8_4_x");                          // 4-byte sequence (codepoint ≥ 0x10000)

    // -- 2-byte UTF-8 (codepoint 0x80..0x7FF) --
    emitter.label("__rt_json_str_utf8_2_x");
    // Bounds check the continuation byte before dereferencing the source
    // pointer; a lead byte at the very end of the input must route to the
    // malformed handler instead of reading past the buffer.
    emitter.instruction("lea r8, [r13 + 1]");                                   // index of the continuation byte
    emitter.instruction("cmp r8, QWORD PTR [rbp - 16]");                        // is the continuation byte within the input?
    emitter.instruction("jae __rt_json_str_utf8_malformed_x");                  // truncated 2-byte sequence → malformed
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source pointer
    emitter.instruction("movzx r15, BYTE PTR [r10 + r13 + 1]");                 // load b2
    // Validate b2 is a continuation byte (0x80..0xBF).
    emitter.instruction("cmp r15, 0x80");                                       // continuation bytes start at 0x80
    emitter.instruction("jb __rt_json_str_utf8_malformed_x");                   // below the continuation range → malformed
    emitter.instruction("cmp r15, 0xC0");                                       // continuation bytes end at 0xBF
    emitter.instruction("jae __rt_json_str_utf8_malformed_x");                  // at/above 0xC0 → malformed
    emitter.instruction("movzx rax, r14b");                                     // widen the lead byte for arithmetic
    emitter.instruction("and rax, 0x1F");                                       // b1 & 0x1F → top 5 bits
    emitter.instruction("shl rax, 6");                                          // shift into bits 6..10
    emitter.instruction("and r15, 0x3F");                                       // b2 & 0x3F → low 6 bits
    emitter.instruction("or rax, r15");                                         // assemble the codepoint in rax
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // park the codepoint for the emit helper
    emitter.instruction("mov r12, 2");                                          // 2 source bytes consumed
    emitter.instruction("jmp __rt_json_str_utf8_emit_bmp_x");                   // emit a single \uXXXX

    // -- 3-byte UTF-8 (codepoint 0x800..0xFFFF) --
    emitter.label("__rt_json_str_utf8_3_x");
    // Bounds check the final byte of the 3-byte sequence; if r13+2 fits,
    // both r13+1 and r13+2 fit too.
    emitter.instruction("lea r8, [r13 + 2]");                                   // index of byte 3 (last byte of the sequence)
    emitter.instruction("cmp r8, QWORD PTR [rbp - 16]");                        // is the last byte within the input?
    emitter.instruction("jae __rt_json_str_utf8_malformed_x");                  // truncated 3-byte sequence → malformed
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source pointer
    emitter.instruction("movzx r15, BYTE PTR [r10 + r13 + 1]");                 // load b2
    // Validate b2 is a continuation byte.
    emitter.instruction("cmp r15, 0x80");                                       // continuation bytes start at 0x80
    emitter.instruction("jb __rt_json_str_utf8_malformed_x");                   // below the continuation range → malformed
    emitter.instruction("cmp r15, 0xC0");                                       // continuation bytes end at 0xBF
    emitter.instruction("jae __rt_json_str_utf8_malformed_x");                  // at/above 0xC0 → malformed
    emitter.instruction("movzx rcx, BYTE PTR [r10 + r13 + 2]");                 // load b3
    // Validate b3 is a continuation byte.
    emitter.instruction("cmp rcx, 0x80");                                       // continuation bytes start at 0x80
    emitter.instruction("jb __rt_json_str_utf8_malformed_x");                   // below the continuation range → malformed
    emitter.instruction("cmp rcx, 0xC0");                                       // continuation bytes end at 0xBF
    emitter.instruction("jae __rt_json_str_utf8_malformed_x");                  // at/above 0xC0 → malformed
    emitter.instruction("movzx rax, r14b");                                     // widen the lead byte
    emitter.instruction("and rax, 0x0F");                                       // b1 & 0x0F → top 4 bits
    emitter.instruction("shl rax, 12");                                         // shift into bits 12..15
    emitter.instruction("and r15, 0x3F");                                       // b2 & 0x3F → middle 6 bits
    emitter.instruction("shl r15, 6");                                          // shift into bits 6..11
    emitter.instruction("or rax, r15");                                         // merge middle bits
    emitter.instruction("and rcx, 0x3F");                                       // b3 & 0x3F → low 6 bits
    emitter.instruction("or rax, rcx");                                         // assemble the codepoint in rax
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // park the codepoint for the emit helper
    emitter.instruction("mov r12, 3");                                          // 3 source bytes consumed
    emitter.instruction("jmp __rt_json_str_utf8_emit_bmp_x");                   // emit a single \uXXXX

    // -- 4-byte UTF-8 (codepoint 0x10000..0x10FFFF) → emit a surrogate pair --
    emitter.label("__rt_json_str_utf8_4_x");
    // Bounds check the final byte of the 4-byte sequence.
    emitter.instruction("lea r8, [r13 + 3]");                                   // index of byte 4 (last byte of the sequence)
    emitter.instruction("cmp r8, QWORD PTR [rbp - 16]");                        // is the last byte within the input?
    emitter.instruction("jae __rt_json_str_utf8_malformed_x");                  // truncated 4-byte sequence → malformed
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source pointer
    emitter.instruction("movzx r15, BYTE PTR [r10 + r13 + 1]");                 // load b2
    // Validate b2 is a continuation byte.
    emitter.instruction("cmp r15, 0x80");                                       // continuation bytes start at 0x80
    emitter.instruction("jb __rt_json_str_utf8_malformed_x");                   // below the continuation range → malformed
    emitter.instruction("cmp r15, 0xC0");                                       // continuation bytes end at 0xBF
    emitter.instruction("jae __rt_json_str_utf8_malformed_x");                  // at/above 0xC0 → malformed
    emitter.instruction("movzx rcx, BYTE PTR [r10 + r13 + 2]");                 // load b3
    // Validate b3 is a continuation byte.
    emitter.instruction("cmp rcx, 0x80");                                       // continuation bytes start at 0x80
    emitter.instruction("jb __rt_json_str_utf8_malformed_x");                   // below the continuation range → malformed
    emitter.instruction("cmp rcx, 0xC0");                                       // continuation bytes end at 0xBF
    emitter.instruction("jae __rt_json_str_utf8_malformed_x");                  // at/above 0xC0 → malformed
    emitter.instruction("movzx r9, BYTE PTR [r10 + r13 + 3]");                  // load b4
    // Validate b4 is a continuation byte.
    emitter.instruction("cmp r9, 0x80");                                        // continuation bytes start at 0x80
    emitter.instruction("jb __rt_json_str_utf8_malformed_x");                   // below the continuation range → malformed
    emitter.instruction("cmp r9, 0xC0");                                        // continuation bytes end at 0xBF
    emitter.instruction("jae __rt_json_str_utf8_malformed_x");                  // at/above 0xC0 → malformed
    emitter.instruction("movzx rax, r14b");                                     // widen the lead byte
    emitter.instruction("and rax, 0x07");                                       // b1 & 0x07 → top 3 bits
    emitter.instruction("shl rax, 18");                                         // shift into bits 18..20
    emitter.instruction("and r15, 0x3F");                                       // b2 & 0x3F → 6 bits
    emitter.instruction("shl r15, 12");                                         // shift into bits 12..17
    emitter.instruction("or rax, r15");                                         // merge bits 12..17
    emitter.instruction("and rcx, 0x3F");                                       // b3 & 0x3F → 6 bits
    emitter.instruction("shl rcx, 6");                                          // shift into bits 6..11
    emitter.instruction("or rax, rcx");                                         // merge bits 6..11
    emitter.instruction("and r9, 0x3F");                                        // b4 & 0x3F → low 6 bits
    emitter.instruction("or rax, r9");                                          // full 21-bit codepoint
    // Compute the surrogate pair: cp -= 0x10000; high = 0xD800 + (cp >> 10);
    // low = 0xDC00 + (cp & 0x3FF).
    emitter.instruction("sub rax, 0x10000");                                    // cp -= 0x10000
    emitter.instruction("mov r15, rax");                                        // copy cp for the high-surrogate computation
    emitter.instruction("shr r15, 10");                                         // (cp >> 10) → high-surrogate index
    emitter.instruction("add r15, 0xD800");                                     // high surrogate codepoint
    emitter.instruction("and rax, 0x3FF");                                      // (cp & 0x3FF) → low-surrogate index
    emitter.instruction("add rax, 0xDC00");                                     // low surrogate codepoint
    emitter.instruction("mov QWORD PTR [rbp - 48], r13");                       // checkpoint the source index across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // park the low surrogate
    emitter.instruction("mov rdi, r15");                                        // pass the high surrogate to the emit helper
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the running write pointer
    emitter.instruction("call __rt_json_str_emit_u16_x");                       // emit \uHHHH for the high surrogate
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the low surrogate
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the running write pointer (now past the high surrogate)
    emitter.instruction("call __rt_json_str_emit_u16_x");                       // emit \uLLLL for the low surrogate
    emitter.instruction("mov r13, QWORD PTR [rbp - 48]");                       // restore the source index
    emitter.instruction("add r13, 4");                                          // advance past the 4-byte UTF-8 sequence
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // save the updated source index
    emitter.instruction("jmp __rt_json_str_loop");                              // continue scanning the remaining source bytes

    // -- emit a BMP codepoint as a single \uXXXX --
    emitter.label("__rt_json_str_utf8_emit_bmp_x");
    emitter.instruction("mov QWORD PTR [rbp - 48], r13");                       // checkpoint the source index across the helper call
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // pass the codepoint to the emit helper
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the running write pointer
    emitter.instruction("call __rt_json_str_emit_u16_x");                       // emit \uXXXX
    emitter.instruction("mov r13, QWORD PTR [rbp - 48]");                       // restore the source index
    emitter.instruction("add r13, r12");                                        // advance past the consumed source bytes
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // save the updated source index
    emitter.instruction("jmp __rt_json_str_loop");                              // continue scanning the remaining source bytes

    // -- malformed UTF-8 handler (x86_64) --
    // Reached from any UTF-8 dispatch site that detected an invalid lead
    // byte, a truncated multi-byte sequence, or a non-continuation trailing
    // byte. Mirrors the ARM64 logic: observe the active flag bitmask, then
    //   * JSON_INVALID_UTF8_IGNORE     (bit 0x100000) — drop the byte;
    //   * JSON_INVALID_UTF8_SUBSTITUTE (bit 0x200000) — emit U+FFFD then
    //     drop the byte;
    //   * neither — record JSON_ERROR_UTF8 (5) through the throw helper so
    //     JsonException is raised when JSON_THROW_ON_ERROR is set, and
    //     fall through to the IGNORE recovery on the no-throw path.
    emitter.label("__rt_json_str_utf8_malformed_x");
    emitter.instruction("mov QWORD PTR [rbp - 48], r13");                       // park the source index across the helper-call sequence
    emitter.instruction("test r15, 0x100000");                                  // JSON_INVALID_UTF8_IGNORE bit (cached flag)
    emitter.instruction("jne __rt_json_str_utf8_skip_byte_x");                  // skip the byte and keep encoding
    emitter.instruction("test r15, 0x200000");                                  // JSON_INVALID_UTF8_SUBSTITUTE bit (cached flag)
    emitter.instruction("jne __rt_json_str_utf8_substitute_byte_x");            // emit U+FFFD then skip the byte
    // Neither sanitization flag is set: report JSON_ERROR_UTF8 and let the
    // throw helper raise when JSON_THROW_ON_ERROR is requested. When it
    // returns (no-throw path), continue with the IGNORE recovery so the
    // encoder still emits well-formed JSON for the rest of the input.
    emitter.instruction("mov rax, 5");                                          // JSON_ERROR_UTF8
    emitter.instruction("call __rt_json_throw_error");                          // record the error and throw when JSON_THROW_ON_ERROR is set
    emitter.instruction("jmp __rt_json_str_utf8_skip_byte_x");                  // fall through to skip the malformed byte

    // -- malformed UTF-8: emit U+FFFD then advance one byte --
    emitter.label("__rt_json_str_utf8_substitute_byte_x");
    emitter.instruction("mov rdi, 0xFFFD");                                     // U+FFFD REPLACEMENT CHARACTER
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the running write pointer
    emitter.instruction("call __rt_json_str_emit_u16_x");                       // emit � into the concat buffer
    // Fall through to skip the malformed byte after substituting.

    // -- malformed UTF-8: skip a single byte and resume the main loop --
    emitter.label("__rt_json_str_utf8_skip_byte_x");
    emitter.instruction("mov r13, QWORD PTR [rbp - 48]");                       // restore the source index parked above
    emitter.instruction("add r13, 1");                                          // advance past exactly one malformed byte (PHP-faithful recovery)
    emitter.instruction("mov QWORD PTR [rbp - 40], r13");                       // persist the updated source index for the next loop iteration
    emitter.instruction("jmp __rt_json_str_loop");                              // resume scanning the remaining source bytes

    // -- helper: write \uXXXX to [r11], advance r11 by 6, persist write pos --
    // Inputs: rdi = 16-bit codepoint, r11 = current write pointer
    emitter.label("__rt_json_str_emit_u16_x");
    emitter.instruction("mov BYTE PTR [r11], 92");                              // emit the backslash prefix
    emitter.instruction("mov BYTE PTR [r11 + 1], 117");                         // emit the unicode marker 'u'
    // Nibble 3 (bits 12-15)
    emitter.instruction("mov rax, rdi");                                        // copy codepoint
    emitter.instruction("shr rax, 12");                                         // extract bits 12..15
    emitter.instruction("and rax, 0xF");                                        // mask to four bits
    emitter.instruction("cmp rax, 10");                                         // is the nibble in 0..9?
    emitter.instruction("jl __rt_json_str_emit_u16_n3_dec_x");                  // decimal-digit branch
    emitter.instruction("add rax, 7");                                          // shift A..F up to ASCII range
    emitter.label("__rt_json_str_emit_u16_n3_dec_x");
    emitter.instruction("add rax, 48");                                         // convert nibble to ASCII digit
    emitter.instruction("mov BYTE PTR [r11 + 2], al");                          // emit the digit
    // Nibble 2 (bits 8-11)
    emitter.instruction("mov rax, rdi");                                        // load or prepare JSON string encoder state
    emitter.instruction("shr rax, 8");                                          // update the JSON string encoder cursor or counter
    emitter.instruction("and rax, 0xF");                                        // update the JSON string encoder cursor or counter
    emitter.instruction("cmp rax, 10");                                         // check the current JSON string encoder condition
    emitter.instruction("jl __rt_json_str_emit_u16_n2_dec_x");                  // branch on the current JSON string encoder condition
    emitter.instruction("add rax, 7");                                          // update the JSON string encoder cursor or counter
    emitter.label("__rt_json_str_emit_u16_n2_dec_x");
    emitter.instruction("add rax, 48");                                         // update the JSON string encoder cursor or counter
    emitter.instruction("mov BYTE PTR [r11 + 3], al");                          // load or prepare JSON string encoder state
    // Nibble 1 (bits 4-7)
    emitter.instruction("mov rax, rdi");                                        // load or prepare JSON string encoder state
    emitter.instruction("shr rax, 4");                                          // update the JSON string encoder cursor or counter
    emitter.instruction("and rax, 0xF");                                        // update the JSON string encoder cursor or counter
    emitter.instruction("cmp rax, 10");                                         // check the current JSON string encoder condition
    emitter.instruction("jl __rt_json_str_emit_u16_n1_dec_x");                  // branch on the current JSON string encoder condition
    emitter.instruction("add rax, 7");                                          // update the JSON string encoder cursor or counter
    emitter.label("__rt_json_str_emit_u16_n1_dec_x");
    emitter.instruction("add rax, 48");                                         // update the JSON string encoder cursor or counter
    emitter.instruction("mov BYTE PTR [r11 + 4], al");                          // load or prepare JSON string encoder state
    // Nibble 0 (bits 0-3)
    emitter.instruction("mov rax, rdi");                                        // load or prepare JSON string encoder state
    emitter.instruction("and rax, 0xF");                                        // update the JSON string encoder cursor or counter
    emitter.instruction("cmp rax, 10");                                         // check the current JSON string encoder condition
    emitter.instruction("jl __rt_json_str_emit_u16_n0_dec_x");                  // branch on the current JSON string encoder condition
    emitter.instruction("add rax, 7");                                          // update the JSON string encoder cursor or counter
    emitter.label("__rt_json_str_emit_u16_n0_dec_x");
    emitter.instruction("add rax, 48");                                         // update the JSON string encoder cursor or counter
    emitter.instruction("mov BYTE PTR [r11 + 5], al");                          // load or prepare JSON string encoder state
    emitter.instruction("add r11, 6");                                          // advance the write pointer past the escape
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // persist the updated write pointer
    emitter.instruction("ret");                                                 // return to the caller

    // -- helper: classify a string as a JSON number per RFC 8259 --
    // Inputs: rax = source ptr, rdx = source length
    // Output: rax = 1 when the entire input matches, 0 otherwise.
    // Clobbers rcx, r9.
    emitter.label("__rt_json_str_is_numeric_x");
    emitter.instruction("xor rcx, rcx");                                        // initialize the source index
    emitter.instruction("test rdx, rdx");                                       // empty string short-circuits to 0
    emitter.instruction("je __rt_json_str_is_numeric_x_fail");                  // branch on the current JSON string encoder condition

    // Optional leading minus sign.
    emitter.instruction("movzx r9, BYTE PTR [rax + rcx]");                      // peek the first byte
    emitter.instruction("cmp r9, 45");                                          // is it '-'?
    emitter.instruction("jne __rt_json_str_is_numeric_x_int_start");            // branch on the current JSON string encoder condition
    emitter.instruction("add rcx, 1");                                          // consume the minus
    emitter.instruction("cmp rcx, rdx");                                        // anything after the minus?
    emitter.instruction("jae __rt_json_str_is_numeric_x_fail");                 // bare '-' is not numeric

    emitter.label("__rt_json_str_is_numeric_x_int_start");
    emitter.instruction("movzx r9, BYTE PTR [rax + rcx]");                      // load the first integer-part byte
    emitter.instruction("sub r9, 48");                                          // ASCII digit → 0..9 if valid
    emitter.instruction("cmp r9, 9");                                           // 0..9 range
    emitter.instruction("ja __rt_json_str_is_numeric_x_fail");                  // first byte must be a digit
    emitter.instruction("add rcx, 1");                                          // advance past the first digit

    emitter.label("__rt_json_str_is_numeric_x_int_loop");
    emitter.instruction("cmp rcx, rdx");                                        // end of input?
    emitter.instruction("jae __rt_json_str_is_numeric_x_ok");                   // pure integer → numeric
    emitter.instruction("movzx r9, BYTE PTR [rax + rcx]");                      // peek
    emitter.instruction("mov r10, r9");                                         // copy for the digit test
    emitter.instruction("sub r10, 48");                                         // digit?
    emitter.instruction("cmp r10, 9");                                          // 0..9
    emitter.instruction("ja __rt_json_str_is_numeric_x_after_int");             // non-digit → check fraction or exponent
    emitter.instruction("add rcx, 1");                                          // consume the digit
    emitter.instruction("jmp __rt_json_str_is_numeric_x_int_loop");             // continue in the JSON string encoder control path

    emitter.label("__rt_json_str_is_numeric_x_after_int");
    emitter.instruction("cmp r9, 46");                                          // '.'?
    emitter.instruction("je __rt_json_str_is_numeric_x_frac_start");            // branch on the current JSON string encoder condition
    emitter.instruction("cmp r9, 101");                                         // 'e'?
    emitter.instruction("je __rt_json_str_is_numeric_x_exp_sign");              // branch on the current JSON string encoder condition
    emitter.instruction("cmp r9, 69");                                          // 'E'?
    emitter.instruction("je __rt_json_str_is_numeric_x_exp_sign");              // branch on the current JSON string encoder condition
    emitter.instruction("jmp __rt_json_str_is_numeric_x_fail");                 // any other byte → not numeric

    emitter.label("__rt_json_str_is_numeric_x_frac_start");
    emitter.instruction("add rcx, 1");                                          // consume the '.'
    emitter.instruction("cmp rcx, rdx");                                        // any digit after?
    emitter.instruction("jae __rt_json_str_is_numeric_x_fail");                 // branch on the current JSON string encoder condition
    emitter.instruction("movzx r9, BYTE PTR [rax + rcx]");                      // peek
    emitter.instruction("sub r9, 48");                                          // update the JSON string encoder cursor or counter
    emitter.instruction("cmp r9, 9");                                           // check the current JSON string encoder condition
    emitter.instruction("ja __rt_json_str_is_numeric_x_fail");                  // need at least one fractional digit
    emitter.instruction("add rcx, 1");                                          // consume the first fractional digit

    emitter.label("__rt_json_str_is_numeric_x_frac_loop");
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON string encoder condition
    emitter.instruction("jae __rt_json_str_is_numeric_x_ok");                   // branch on the current JSON string encoder condition
    emitter.instruction("movzx r9, BYTE PTR [rax + rcx]");                      // load or prepare JSON string encoder state
    emitter.instruction("mov r10, r9");                                         // load or prepare JSON string encoder state
    emitter.instruction("sub r10, 48");                                         // update the JSON string encoder cursor or counter
    emitter.instruction("cmp r10, 9");                                          // check the current JSON string encoder condition
    emitter.instruction("ja __rt_json_str_is_numeric_x_after_frac");            // branch on the current JSON string encoder condition
    emitter.instruction("add rcx, 1");                                          // update the JSON string encoder cursor or counter
    emitter.instruction("jmp __rt_json_str_is_numeric_x_frac_loop");            // continue in the JSON string encoder control path

    emitter.label("__rt_json_str_is_numeric_x_after_frac");
    emitter.instruction("cmp r9, 101");                                         // 'e'?
    emitter.instruction("je __rt_json_str_is_numeric_x_exp_sign");              // branch on the current JSON string encoder condition
    emitter.instruction("cmp r9, 69");                                          // 'E'?
    emitter.instruction("je __rt_json_str_is_numeric_x_exp_sign");              // branch on the current JSON string encoder condition
    emitter.instruction("jmp __rt_json_str_is_numeric_x_fail");                 // continue in the JSON string encoder control path

    emitter.label("__rt_json_str_is_numeric_x_exp_sign");
    emitter.instruction("add rcx, 1");                                          // consume the 'e'/'E'
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON string encoder condition
    emitter.instruction("jae __rt_json_str_is_numeric_x_fail");                 // bare 'Xe' is not numeric
    emitter.instruction("movzx r9, BYTE PTR [rax + rcx]");                      // load or prepare JSON string encoder state
    emitter.instruction("cmp r9, 43");                                          // optional '+'
    emitter.instruction("je __rt_json_str_is_numeric_x_exp_advance_sign");      // branch on the current JSON string encoder condition
    emitter.instruction("cmp r9, 45");                                          // optional '-'
    emitter.instruction("je __rt_json_str_is_numeric_x_exp_advance_sign");      // branch on the current JSON string encoder condition
    emitter.instruction("jmp __rt_json_str_is_numeric_x_exp_first_digit");      // continue in the JSON string encoder control path
    emitter.label("__rt_json_str_is_numeric_x_exp_advance_sign");
    emitter.instruction("add rcx, 1");                                          // consume the exponent sign
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON string encoder condition
    emitter.instruction("jae __rt_json_str_is_numeric_x_fail");                 // bare 'e+' / 'e-' is not numeric

    emitter.label("__rt_json_str_is_numeric_x_exp_first_digit");
    emitter.instruction("movzx r9, BYTE PTR [rax + rcx]");                      // load or prepare JSON string encoder state
    emitter.instruction("sub r9, 48");                                          // update the JSON string encoder cursor or counter
    emitter.instruction("cmp r9, 9");                                           // check the current JSON string encoder condition
    emitter.instruction("ja __rt_json_str_is_numeric_x_fail");                  // need at least one exponent digit
    emitter.instruction("add rcx, 1");                                          // update the JSON string encoder cursor or counter

    emitter.label("__rt_json_str_is_numeric_x_exp_loop");
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON string encoder condition
    emitter.instruction("jae __rt_json_str_is_numeric_x_ok");                   // branch on the current JSON string encoder condition
    emitter.instruction("movzx r9, BYTE PTR [rax + rcx]");                      // load or prepare JSON string encoder state
    emitter.instruction("sub r9, 48");                                          // update the JSON string encoder cursor or counter
    emitter.instruction("cmp r9, 9");                                           // check the current JSON string encoder condition
    emitter.instruction("ja __rt_json_str_is_numeric_x_fail");                  // any non-digit after exponent digits → not numeric
    emitter.instruction("add rcx, 1");                                          // update the JSON string encoder cursor or counter
    emitter.instruction("jmp __rt_json_str_is_numeric_x_exp_loop");             // continue in the JSON string encoder control path

    emitter.label("__rt_json_str_is_numeric_x_ok");
    emitter.instruction("mov rax, 1");                                          // signal numeric
    emitter.instruction("ret");                                                 // return from the JSON string encoder helper
    emitter.label("__rt_json_str_is_numeric_x_fail");
    emitter.instruction("xor rax, rax");                                        // signal non-numeric
    emitter.instruction("ret");                                                 // return from the JSON string encoder helper

    emitter.label("__rt_json_str_close");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the concat-buffer write pointer after the final escaped payload byte
    emitter.instruction("mov BYTE PTR [r11], 34");                              // append the closing JSON quote to complete the encoded string slice
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the closing quote
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the encoded-string start pointer in the leading x86_64 string result register
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer write pointer before turning it into a slice length
    emitter.instruction("sub rdx, rax");                                        // compute the final encoded-string length from write_end - write_start
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the global offset update
    emitter.instruction("mov rcx, r11");                                        // copy the final concat-buffer write pointer before converting it into an absolute offset
    emitter.instruction("sub rcx, r10");                                        // compute the new absolute concat-buffer offset after the encoded JSON string
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the updated concat-buffer offset so nested writers append after this JSON string
    emitter.instruction("mov r15, QWORD PTR [rbp - 80]");                       // restore the callee-saved register
    emitter.instruction("add rsp, 80");                                         // release the local JSON-string scratch frame before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the encoded JSON string slice in the x86_64 string result registers
}
