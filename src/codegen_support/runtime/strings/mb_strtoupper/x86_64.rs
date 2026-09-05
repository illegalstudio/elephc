//! Purpose:
//! Emits the Linux x86_64 `__rt_mb_strtoupper` helper.
//!
//! Called from:
//! - `super::emit_mb_strtoupper()`.
//!
//! Key details:
//! - Encoding dispatch matches `mb_strlen`: omitted/null and UTF-8/UTF8 use the UTF-8
//!   walker, `8bit`/`binary`/`7bit` are ASCII-only, and other names go through iconv.
//! - UTF-8 uses Unicode full case mapping and copies malformed bytes through unchanged.
//! - `push rbp` plus `sub rsp, 256` keeps framed `call`s 16-byte aligned. Frameless
//!   private helpers (`ensure`, `iconv_free_temps`) pad with `sub rsp, 8` around nested
//!   calls so the SysV audit walks them as aligned when entered in isolation. The
//!   unknown-encoding path uses `mov rsp, rbp` / `pop rbp` instead of `leave` because
//!   the audit models those writes and not `leave`.

use crate::codegen_support::{
    abi,
    emit::Emitter,
    runtime::{arrays::value_error, data::MB_STRTOUPPER_UNKNOWN_ENCODING_MSG},
};

/// Maximum explicit encoding-name length copied into the runtime's stack buffer.
const MAX_ENCODING_NAME_LEN: usize = 63;

/// Emits `__rt_mb_strtoupper(str_ptr, str_len, encoding_ptr, encoding_len) -> (ptr, len)`.
pub(super) fn emit_mb_strtoupper_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_strtoupper (encoding-aware Unicode uppercase) ---");
    emitter.label_global("__rt_mb_strtoupper");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across libc and runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned helper frame
    emitter.instruction("sub rsp, 256");                                        // reserve dest state, encoding-name, case-out, and iconv slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source string length
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // source byte index starts at zero
    emitter.instruction("test r8, r8");                                         // omitted/null encoding is represented by a null pointer
    emitter.instruction("jz __rt_mb_strtoupper_utf8_x86");                      // use the default UTF-8 walker when encoding is omitted/null
    emitter.instruction(&format!("cmp r9, {}", MAX_ENCODING_NAME_LEN));         // does the encoding name fit the stack C-string buffer?
    emitter.instruction("ja __rt_mb_strtoupper_unknown_encoding_x86");          // reject names longer than every PHP-supported alias

    emitter.instruction("lea rdi, [rbp - 256]");                                // destination is the 64-byte encoding-name buffer
    emitter.instruction("xor rcx, rcx");                                        // copied-byte index starts at zero
    emitter.label("__rt_mb_strtoupper_encoding_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied the whole explicit encoding name?
    emitter.instruction("jae __rt_mb_strtoupper_encoding_copied_x86");          // terminate the C string once every byte is copied
    emitter.instruction("mov r10b, BYTE PTR [r8 + rcx]");                       // load one encoding-name byte from the PHP string
    emitter.instruction("mov BYTE PTR [rdi + rcx], r10b");                      // append the byte to the stack C string
    emitter.instruction("inc rcx");                                             // advance the encoding-name byte index
    emitter.instruction("jmp __rt_mb_strtoupper_encoding_copy_x86");            // continue copying the remaining encoding-name bytes
    emitter.label("__rt_mb_strtoupper_encoding_copied_x86");
    emitter.instruction("mov BYTE PTR [rdi + r9], 0");                          // NUL-terminate the explicit encoding name

    emitter.instruction("lea rdi, [rbp - 256]");                                // first strcasecmp argument is the copied encoding name
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_utf8_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with UTF-8 case-insensitively
    emitter.instruction("test eax, eax");                                       // did the encoding match UTF-8?
    emitter.instruction("jz __rt_mb_strtoupper_utf8_x86");                      // UTF-8 uses the validated Unicode walker
    emitter.instruction("lea rdi, [rbp - 256]");                                // reload the copied encoding name after strcasecmp
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_utf8_alias");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with PHP's UTF8 alias
    emitter.instruction("test eax, eax");                                       // did the encoding match UTF8?
    emitter.instruction("jz __rt_mb_strtoupper_utf8_x86");                      // the UTF8 alias uses the same walker
    emitter.instruction("lea rdi, [rbp - 256]");                                // reload the copied encoding name for the byte encodings
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_8bit_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with 8bit
    emitter.instruction("test eax, eax");                                       // did the encoding match 8bit?
    emitter.instruction("jz __rt_mb_strtoupper_bytes_x86");                     // 8bit is ASCII-only uppercase
    emitter.instruction("lea rdi, [rbp - 256]");                                // reload the copied encoding name for the binary alias
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_binary_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with binary
    emitter.instruction("test eax, eax");                                       // did the encoding match binary?
    emitter.instruction("jz __rt_mb_strtoupper_bytes_x86");                     // binary is PHP's alias for 8bit
    emitter.instruction("lea rdi, [rbp - 256]");                                // reload the copied encoding name for the 7bit encoding
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_7bit_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with 7bit
    emitter.instruction("test eax, eax");                                       // did the encoding match 7bit?
    emitter.instruction("jz __rt_mb_strtoupper_bytes_x86");                     // 7bit preserves PHP's one-character-per-byte ASCII fold
    emitter.instruction("jmp __rt_mb_strtoupper_iconv_x86");                    // remaining names decode through iconv

    emit_utf8_x86_64(emitter);
    emit_bytes_x86_64(emitter);
    emit_iconv_x86_64(emitter);

    emitter.label("__rt_mb_strtoupper_empty_x86");
    abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset
    abi::emit_symbol_address(emitter, "r8", "_concat_buf");
    emitter.instruction("lea rax, [r8 + r9]");                                  // empty results still need a valid pointer
    emitter.instruction("xor rdx, rdx");                                        // empty uppercase result has length zero
    emitter.instruction("leave");                                               // release the helper frame and restore rbp
    emitter.instruction("ret");                                                 // return the empty string

    emitter.label("__rt_mb_strtoupper_finish_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // result pointer is the reserved destination
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // result length is the number of bytes written
    emitter.instruction("call __rt_concat_publish");                            // advance concat scratch only for scratch-backed results
    emitter.instruction("leave");                                               // release the helper frame and restore rbp
    emitter.instruction("ret");                                                 // return the uppercase string

    emitter.label("__rt_mb_strtoupper_unknown_encoding_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the 256-byte frame (walker models this; `leave` does not)
    emitter.instruction("pop rbp");                                             // restore the caller frame before the ValueError sequence
    value_error::emit_throw_value_error_x86_64(
        emitter,
        "_mb_strtoupper_unknown_encoding_msg",
        MB_STRTOUPPER_UNKNOWN_ENCODING_MSG.len(),
    );

    emit_helpers_x86_64(emitter);
}

/// Emits the UTF-8 Unicode uppercase walker.
fn emit_utf8_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtoupper_utf8_x86");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // load the source length for the empty-string fast path
    emitter.instruction("test rdx, rdx");                                       // is the source empty?
    emitter.instruction("jz __rt_mb_strtoupper_empty_x86");                     // empty input produces an empty result
    emitter.instruction("mov rax, rdx");                                        // start the reservation from the source length
    emitter.instruction("shl rax, 3");                                          // reserve eight times the source length for full-mapping growth
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // remember the reserved destination capacity
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the destination pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // no uppercase bytes have been written yet

    emitter.label("__rt_mb_strtoupper_utf8_loop_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // load the current source byte index
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // load the source length
    emitter.instruction("cmp r8, rdx");                                         // processed every source byte?
    emitter.instruction("jae __rt_mb_strtoupper_finish_x86");                   // publish once the source is exhausted
    emitter.instruction("call __rt_mb_strtoupper_ensure_x86");                  // keep twelve free destination bytes for the next scalar
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the source pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the source byte index
    emitter.instruction("movzx r9d, BYTE PTR [rsi + r8]");                      // load the next possible UTF-8 leading byte
    emitter.instruction("cmp r9d, 0x80");                                       // ASCII bytes are complete one-byte characters
    emitter.instruction("jb __rt_mb_strtoupper_utf8_ascii_x86");                // uppercase one ASCII byte
    emitter.instruction("cmp r9d, 0xC2");                                       // C0/C1 and continuation bytes are malformed leaders
    emitter.instruction("jb __rt_mb_strtoupper_utf8_invalid_x86");              // copy one malformed byte through
    emitter.instruction("cmp r9d, 0xE0");                                       // C2-DF introduce two-byte sequences
    emitter.instruction("jb __rt_mb_strtoupper_utf8_two_x86");                  // validate a two-byte character
    emitter.instruction("cmp r9d, 0xF0");                                       // E0-EF introduce three-byte sequences
    emitter.instruction("jb __rt_mb_strtoupper_utf8_three_x86");                // validate a three-byte character
    emitter.instruction("cmp r9d, 0xF5");                                       // F0-F4 introduce Unicode-range four-byte sequences
    emitter.instruction("jb __rt_mb_strtoupper_utf8_four_x86");                 // validate a four-byte character
    emitter.instruction("jmp __rt_mb_strtoupper_utf8_invalid_x86");             // F5-FF cannot begin valid UTF-8

    emitter.label("__rt_mb_strtoupper_utf8_ascii_x86");
    emitter.instruction("mov eax, r9d");                                        // ASCII code point is the loaded byte
    emitter.instruction("mov r11, 1");                                          // one source byte is consumed
    emitter.instruction("jmp __rt_mb_strtoupper_utf8_map_x86");                 // apply Unicode uppercase to the ASCII scalar

    emitter.label("__rt_mb_strtoupper_utf8_two_x86");
    emitter.instruction("mov r10, rdx");                                        // copy the total byte length to compute remaining bytes
    emitter.instruction("sub r10, r8");                                         // compute bytes remaining from the two-byte leader
    emitter.instruction("cmp r10, 2");                                          // is the sequence truncated before its continuation byte?
    emitter.instruction("jb __rt_mb_strtoupper_utf8_invalid_x86");              // copy a truncated leader through unchanged
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 1]");                 // load the two-byte sequence continuation
    emitter.instruction("mov ecx, r11d");                                       // preserve the continuation value while checking its prefix
    emitter.instruction("and ecx, 0xC0");                                       // isolate the continuation-byte prefix
    emitter.instruction("cmp ecx, 0x80");                                       // does the second byte have the required 10xxxxxx shape?
    emitter.instruction("jne __rt_mb_strtoupper_utf8_invalid_x86");             // copy the malformed leader through unchanged
    emitter.instruction("mov eax, r9d");                                        // start the two-byte code point from the leader
    emitter.instruction("and eax, 0x1F");                                       // isolate the leader payload
    emitter.instruction("shl eax, 6");                                          // shift the leader payload into place
    emitter.instruction("and r11d, 0x3F");                                      // isolate the continuation payload
    emitter.instruction("or eax, r11d");                                        // assemble the two-byte scalar
    emitter.instruction("mov r11, 2");                                          // two source bytes are consumed
    emitter.instruction("jmp __rt_mb_strtoupper_utf8_map_x86");                 // apply Unicode uppercase

    emitter.label("__rt_mb_strtoupper_utf8_three_x86");
    emitter.instruction("mov r10, rdx");                                        // copy the total byte length to compute remaining bytes
    emitter.instruction("sub r10, r8");                                         // compute bytes remaining from the three-byte leader
    emitter.instruction("cmp r10, 3");                                          // are all two continuation bytes available?
    emitter.instruction("jb __rt_mb_strtoupper_utf8_invalid_x86");              // copy a truncated leader through unchanged
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 1]");                 // load the first three-byte continuation
    emitter.instruction("mov ecx, r11d");                                       // preserve the continuation value while checking its prefix
    emitter.instruction("and ecx, 0xC0");                                       // isolate its continuation-byte prefix
    emitter.instruction("cmp ecx, 0x80");                                       // is the first continuation structurally valid?
    emitter.instruction("jne __rt_mb_strtoupper_utf8_invalid_x86");             // copy the malformed leader through unchanged
    emitter.instruction("cmp r9d, 0xE0");                                       // E0 requires a second byte at least A0 to avoid overlong UTF-8
    emitter.instruction("jne __rt_mb_strtoupper_utf8_three_not_e0_x86");        // skip the E0 lower-bound check for other leaders
    emitter.instruction("cmp r11d, 0xA0");                                      // is the E0 continuation inside the non-overlong range?
    emitter.instruction("jb __rt_mb_strtoupper_utf8_invalid_x86");              // copy an overlong three-byte sequence through
    emitter.label("__rt_mb_strtoupper_utf8_three_not_e0_x86");
    emitter.instruction("cmp r9d, 0xED");                                       // ED requires a second byte below A0 to exclude UTF-16 surrogates
    emitter.instruction("jne __rt_mb_strtoupper_utf8_three_second_x86");        // skip the surrogate bound for other leaders
    emitter.instruction("cmp r11d, 0xA0");                                      // does the ED continuation enter the surrogate range?
    emitter.instruction("jae __rt_mb_strtoupper_utf8_invalid_x86");             // copy UTF-8 encodings of surrogate code points through
    emitter.label("__rt_mb_strtoupper_utf8_three_second_x86");
    emitter.instruction("movzx ecx, BYTE PTR [rsi + r8 + 2]");                  // load the final three-byte continuation
    emitter.instruction("mov r10d, ecx");                                       // preserve the final continuation while checking its prefix
    emitter.instruction("and r10d, 0xC0");                                      // isolate its continuation-byte prefix
    emitter.instruction("cmp r10d, 0x80");                                      // is the final continuation structurally valid?
    emitter.instruction("jne __rt_mb_strtoupper_utf8_invalid_x86");             // copy the malformed leader through unchanged
    emitter.instruction("mov eax, r9d");                                        // start the three-byte code point from the leader
    emitter.instruction("and eax, 0x0F");                                       // isolate the leader payload
    emitter.instruction("shl eax, 12");                                         // shift the leader payload into place
    emitter.instruction("and r11d, 0x3F");                                      // isolate the first continuation payload
    emitter.instruction("shl r11d, 6");                                         // shift the first continuation payload into place
    emitter.instruction("or eax, r11d");                                        // merge the first continuation
    emitter.instruction("and ecx, 0x3F");                                       // isolate the final continuation payload
    emitter.instruction("or eax, ecx");                                         // assemble the three-byte scalar
    emitter.instruction("mov r11, 3");                                          // three source bytes are consumed
    emitter.instruction("jmp __rt_mb_strtoupper_utf8_map_x86");                 // apply Unicode uppercase

    emitter.label("__rt_mb_strtoupper_utf8_four_x86");
    emitter.instruction("mov r10, rdx");                                        // copy the total byte length to compute remaining bytes
    emitter.instruction("sub r10, r8");                                         // compute bytes remaining from the four-byte leader
    emitter.instruction("cmp r10, 4");                                          // are all three continuation bytes available?
    emitter.instruction("jb __rt_mb_strtoupper_utf8_invalid_x86");              // copy a truncated leader through unchanged
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 1]");                 // load the first four-byte continuation
    emitter.instruction("mov ecx, r11d");                                       // preserve the continuation value while checking its prefix
    emitter.instruction("and ecx, 0xC0");                                       // isolate its continuation-byte prefix
    emitter.instruction("cmp ecx, 0x80");                                       // is the first continuation structurally valid?
    emitter.instruction("jne __rt_mb_strtoupper_utf8_invalid_x86");             // copy the malformed leader through unchanged
    emitter.instruction("cmp r9d, 0xF0");                                       // F0 requires a second byte at least 90 to avoid overlong UTF-8
    emitter.instruction("jne __rt_mb_strtoupper_utf8_four_not_f0_x86");         // skip the F0 lower-bound check for other leaders
    emitter.instruction("cmp r11d, 0x90");                                      // is the F0 continuation inside the non-overlong range?
    emitter.instruction("jb __rt_mb_strtoupper_utf8_invalid_x86");              // copy an overlong four-byte sequence through
    emitter.label("__rt_mb_strtoupper_utf8_four_not_f0_x86");
    emitter.instruction("cmp r9d, 0xF4");                                       // F4 requires a second byte below 90 for Unicode's maximum scalar
    emitter.instruction("jne __rt_mb_strtoupper_utf8_four_rest_x86");           // skip the upper bound for F0-F3
    emitter.instruction("cmp r11d, 0x90");                                      // does the F4 continuation exceed U+10FFFF?
    emitter.instruction("jae __rt_mb_strtoupper_utf8_invalid_x86");             // copy out-of-range four-byte sequences through
    emitter.label("__rt_mb_strtoupper_utf8_four_rest_x86");
    emitter.instruction("movzx ecx, BYTE PTR [rsi + r8 + 2]");                  // load the second four-byte continuation
    emitter.instruction("mov r10d, ecx");                                       // preserve the second continuation while checking its prefix
    emitter.instruction("and r10d, 0xC0");                                      // isolate its continuation-byte prefix
    emitter.instruction("cmp r10d, 0x80");                                      // is the second continuation structurally valid?
    emitter.instruction("jne __rt_mb_strtoupper_utf8_invalid_x86");             // copy the malformed leader through unchanged
    emitter.instruction("movzx r10d, BYTE PTR [rsi + r8 + 3]");                 // load the final four-byte continuation
    emitter.instruction("mov edx, r10d");                                       // preserve the final continuation while checking its prefix
    emitter.instruction("and edx, 0xC0");                                       // isolate its continuation-byte prefix
    emitter.instruction("cmp edx, 0x80");                                       // is the final continuation structurally valid?
    emitter.instruction("jne __rt_mb_strtoupper_utf8_invalid_x86");             // copy the malformed leader through unchanged
    emitter.instruction("mov eax, r9d");                                        // start the four-byte code point from the leader
    emitter.instruction("and eax, 0x07");                                       // isolate the leader payload
    emitter.instruction("shl eax, 18");                                         // shift the leader payload into place
    emitter.instruction("and r11d, 0x3F");                                      // isolate the first continuation payload
    emitter.instruction("shl r11d, 12");                                        // shift the first continuation payload into place
    emitter.instruction("or eax, r11d");                                        // merge the first continuation
    emitter.instruction("and ecx, 0x3F");                                       // isolate the second continuation payload
    emitter.instruction("shl ecx, 6");                                          // shift the second continuation payload into place
    emitter.instruction("or eax, ecx");                                         // merge the second continuation
    emitter.instruction("and r10d, 0x3F");                                      // isolate the final continuation payload
    emitter.instruction("or eax, r10d");                                        // assemble the four-byte scalar
    emitter.instruction("mov r11, 4");                                          // four source bytes are consumed
    emitter.instruction("jmp __rt_mb_strtoupper_utf8_map_x86");                 // apply Unicode uppercase

    emitter.label("__rt_mb_strtoupper_utf8_map_x86");
    emitter.instruction("mov QWORD PTR [rbp - 136], r11");                      // preserve the consumed source-byte count across the lookup
    emitter.instruction("lea rdi, [rbp - 184]");                                // case-map destination is the 16-byte stack buffer
    emitter.instruction("call __rt_mb_case_upper");                             // expand the scalar through Unicode full uppercase
    emitter.instruction("mov QWORD PTR [rbp - 192], rax");                      // preserve the number of uppercase scalars
    emitter.instruction("mov QWORD PTR [rbp - 200], 0");                        // uppercase-scalar index starts at zero
    emitter.label("__rt_mb_strtoupper_utf8_encode_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 200]");                       // load the uppercase-scalar index
    emitter.instruction("cmp r8, QWORD PTR [rbp - 192]");                       // encoded every uppercase scalar?
    emitter.instruction("jae __rt_mb_strtoupper_utf8_mapped_x86");              // consume the source sequence after encoding
    emitter.instruction("lea r9, [rbp - 184]");                                 // case-map output buffer
    emitter.instruction("mov eax, DWORD PTR [r9 + r8 * 4]");                    // load the next uppercase scalar
    emitter.instruction("call __rt_mb_strtoupper_put_utf8_x86");                // append its UTF-8 encoding to the destination
    emitter.instruction("inc QWORD PTR [rbp - 200]");                           // advance the uppercase-scalar index
    emitter.instruction("jmp __rt_mb_strtoupper_utf8_encode_x86");              // encode the remaining uppercase scalars
    emitter.label("__rt_mb_strtoupper_utf8_mapped_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 136]");                      // restore the consumed source-byte count
    emitter.instruction("add QWORD PTR [rbp - 24], r11");                       // consume the mapped source sequence
    emitter.instruction("jmp __rt_mb_strtoupper_utf8_loop_x86");                // continue scanning the remaining bytes

    emitter.label("__rt_mb_strtoupper_utf8_invalid_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // load the destination pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // load the number of bytes already written
    emitter.instruction("mov BYTE PTR [r8 + r10], r9b");                        // copy the malformed byte through unchanged
    emitter.instruction("inc QWORD PTR [rbp - 48]");                            // account for the copied malformed byte
    emitter.instruction("inc QWORD PTR [rbp - 24]");                            // consume the malformed byte
    emitter.instruction("jmp __rt_mb_strtoupper_utf8_loop_x86");                // continue scanning the remaining bytes
}

/// Emits ASCII-only uppercase for PHP's 8bit/binary/7bit encodings.
fn emit_bytes_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtoupper_bytes_x86");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // load the source length
    emitter.instruction("test rdx, rdx");                                       // is the source empty?
    emitter.instruction("jz __rt_mb_strtoupper_empty_x86");                     // empty input produces an empty result
    emitter.instruction("mov rax, rdx");                                        // byte encodings never grow
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // remember the reserved destination capacity
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the destination pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the source pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the source length
    emitter.instruction("xor rcx, rcx");                                        // byte index starts at zero
    emitter.label("__rt_mb_strtoupper_bytes_loop_x86");
    emitter.instruction("cmp rcx, rdx");                                        // copied every source byte?
    emitter.instruction("jae __rt_mb_strtoupper_bytes_done_x86");               // publish once the source is exhausted
    emitter.instruction("movzx r8d, BYTE PTR [rsi + rcx]");                     // load the next source byte
    emitter.instruction("cmp r8d, 97");                                         // compare with 'a'
    emitter.instruction("jb __rt_mb_strtoupper_bytes_store_x86");               // bytes below 'a' stay unchanged
    emitter.instruction("cmp r8d, 122");                                        // compare with 'z'
    emitter.instruction("ja __rt_mb_strtoupper_bytes_store_x86");               // bytes above 'z' stay unchanged
    emitter.instruction("sub r8d, 32");                                         // convert a-z to A-Z
    emitter.label("__rt_mb_strtoupper_bytes_store_x86");
    emitter.instruction("mov BYTE PTR [rax + rcx], r8b");                       // store the possibly uppercased byte
    emitter.instruction("inc rcx");                                             // advance the byte index
    emitter.instruction("jmp __rt_mb_strtoupper_bytes_loop_x86");               // continue the ASCII fold
    emitter.label("__rt_mb_strtoupper_bytes_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // the result length equals the source length
    emitter.instruction("jmp __rt_mb_strtoupper_finish_x86");                   // publish the ASCII-uppercased copy
}

/// Emits iconv-backed uppercase for encodings other than UTF-8 and the byte aliases.
fn emit_iconv_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtoupper_iconv_x86");
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // no uppercase temporary exists until mapping allocates one
    abi::emit_symbol_address(emitter, "rdi", "_mb_strlen_utf32le_name");
    emitter.instruction("lea rsi, [rbp - 256]");                                // iconv source encoding is the copied explicit name
    emitter.instruction("call iconv_open");                                     // create the encoding-to-UTF-32LE conversion descriptor
    emitter.instruction("cmp rax, -1");                                         // did iconv_open return the failure sentinel?
    emitter.instruction("je __rt_mb_strtoupper_unknown_encoding_x86");          // unknown encoding names raise PHP's ValueError
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // preserve the decode descriptor
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the source length
    emitter.instruction("inc rax");                                             // keep one extra UTF-32 slot for a possible BOM
    emitter.instruction("shl rax, 2");                                          // four bytes per decoded scalar
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // remember the decode-buffer capacity
    emitter.instruction("call __rt_heap_alloc");                                // allocate a temporary UTF-32 decode buffer
    emitter.instruction("mov QWORD PTR [rax - 8], 1");                          // stamp the temporary so heap_free can release it
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the decode buffer
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // decoded UTF-32 byte count starts at zero
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // iconv input starts at the PHP string
    emitter.instruction("mov QWORD PTR [rbp - 104], r8");                       // initialize iconv's mutable input pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // iconv still has the whole source to consume
    emitter.instruction("mov QWORD PTR [rbp - 112], r8");                       // initialize iconv's mutable input-byte count

    emitter.label("__rt_mb_strtoupper_iconv_dec_loop_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 112], 0");                        // are any input bytes still undecoded?
    emitter.instruction("je __rt_mb_strtoupper_iconv_decoded_x86");             // decode is finished when input is exhausted
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // decode-buffer base
    emitter.instruction("add r8, QWORD PTR [rbp - 72]");                        // current UTF-32 write cursor
    emitter.instruction("mov QWORD PTR [rbp - 120], r8");                       // initialize iconv's mutable output pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // decode-buffer capacity
    emitter.instruction("sub r8, QWORD PTR [rbp - 72]");                        // remaining output bytes
    emitter.instruction("jz __rt_mb_strtoupper_iconv_dec_grow_x86");            // grow when the temporary is already full
    emitter.instruction("mov QWORD PTR [rbp - 128], r8");                       // initialize iconv's mutable output-byte count
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // iconv argument 0 is the decode descriptor
    emitter.instruction("lea rsi, [rbp - 104]");                                // iconv argument 1 is `&input_ptr`
    emitter.instruction("lea rdx, [rbp - 112]");                                // iconv argument 2 is `&input_bytes_left`
    emitter.instruction("lea rcx, [rbp - 120]");                                // iconv argument 3 is `&output_ptr`
    emitter.instruction("lea r8, [rbp - 128]");                                 // iconv argument 4 is `&output_bytes_left`
    emitter.instruction("call iconv");                                          // decode as many complete characters as fit
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // reload the decode-buffer capacity
    emitter.instruction("sub r8, QWORD PTR [rbp - 128]");                       // written = capacity - unused
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // persist the accumulated UTF-32 byte count
    emitter.instruction("cmp rax, -1");                                         // did iconv report an error condition?
    emitter.instruction("jne __rt_mb_strtoupper_iconv_dec_loop_x86");           // successful progress continues
    emitter.instruction("cmp QWORD PTR [rbp - 128], 0");                        // did iconv merely fill the output scratch?
    emitter.instruction("je __rt_mb_strtoupper_iconv_dec_grow_x86");            // a full output buffer needs a larger decode temporary
    emitter.instruction("call __errno_location");                               // fetch the Linux thread-local errno written by iconv
    emitter.instruction("cmp DWORD PTR [rax], 22");                             // EINVAL means a truncated final sequence
    emitter.instruction("je __rt_mb_strtoupper_iconv_decoded_x86");             // stop at a truncated suffix without inventing scalars
    emitter.instruction("cmp QWORD PTR [rbp - 112], 0");                        // are bytes still present at the malformed sequence?
    emitter.instruction("je __rt_mb_strtoupper_iconv_decoded_x86");             // defensive completion
    emitter.instruction("add QWORD PTR [rbp - 104], 1");                        // skip one malformed input byte
    emitter.instruction("sub QWORD PTR [rbp - 112], 1");                        // remove the malformed byte
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // reload the decode descriptor
    emitter.instruction("xor rsi, rsi");                                        // reset iconv shift state
    emitter.instruction("xor rdx, rdx");                                        // no input participates in the reset
    emitter.instruction("xor rcx, rcx");                                        // no output participates in the reset
    emitter.instruction("xor r8, r8");                                          // no output count participates in the reset
    emitter.instruction("call iconv");                                          // reset stateful decoders after skipping one malformed byte
    emitter.instruction("jmp __rt_mb_strtoupper_iconv_dec_loop_x86");           // continue decoding

    emitter.label("__rt_mb_strtoupper_iconv_dec_grow_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // current decode-buffer capacity
    emitter.instruction("shl rax, 1");                                          // double the UTF-32 temporary
    emitter.instruction("add rax, 16");                                         // plus a small extra so a zero-capacity buffer can grow
    emitter.instruction("mov QWORD PTR [rbp - 160], rax");                      // persist the new capacity across the allocator call
    emitter.instruction("call __rt_heap_alloc");                                // allocate the grown decode buffer
    emitter.instruction("mov QWORD PTR [rax - 8], 1");                          // stamp the grown decode temporary
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // old decode buffer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                       // bytes already decoded
    emitter.instruction("mov rdi, rax");                                        // grown decode buffer is the copy destination
    emitter.instruction("cld");                                                 // `rep movsb` requires a forward copy
    emitter.instruction("rep movsb");                                           // duplicate the decoded prefix
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // persist the grown decode buffer
    emitter.instruction("mov r8, QWORD PTR [rbp - 160]");                       // reload the new capacity
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // persist the grown capacity
    emitter.instruction("mov rax, rsi");                                        // release the superseded decode buffer
    emitter.instruction("sub rax, QWORD PTR [rbp - 72]");                       // `rep movsb` advanced rsi past the copied prefix
    emitter.instruction("call __rt_heap_free");                                 // return the old temporary to the heap
    emitter.instruction("jmp __rt_mb_strtoupper_iconv_dec_loop_x86");           // retry the conversion with the larger buffer

    emitter.label("__rt_mb_strtoupper_iconv_decoded_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // close the decode descriptor
    emitter.instruction("call iconv_close");                                    // release the encoding-to-UTF-32LE descriptor
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // allocate 3x the decoded UTF-32 for 1:N growth
    emitter.instruction("lea rax, [rax + rax * 2]");                            // dest_cap = decoded_len * 3
    emitter.instruction("cmp rax, 16");                                         // keep a minimum uppercase buffer
    emitter.instruction("jae __rt_mb_strtoupper_iconv_upper_cap_x86");          // use the 3x size when it is large enough
    emitter.instruction("mov rax, 16");                                         // minimum 16-byte uppercase buffer
    emitter.label("__rt_mb_strtoupper_iconv_upper_cap_x86");
    emitter.instruction("mov QWORD PTR [rbp - 160], rax");                      // remember the uppercase-buffer capacity
    emitter.instruction("call __rt_heap_alloc");                                // allocate the uppercase UTF-32 buffer
    emitter.instruction("mov QWORD PTR [rax - 8], 1");                          // stamp the uppercase temporary
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the uppercase buffer
    emitter.instruction("mov QWORD PTR [rbp - 152], 0");                        // uppercase written byte count starts at zero
    emitter.instruction("mov QWORD PTR [rbp - 136], 0");                        // decode byte index starts at zero
    emitter.instruction("mov r8, QWORD PTR [rbp - 72]");                        // decoded UTF-32 byte count
    emitter.instruction("mov QWORD PTR [rbp - 144], r8");                       // persist the decode length for the mapping loop
    emitter.instruction("mov QWORD PTR [rbp - 168], 0");                        // saw_payload starts false so a leading BOM can be skipped

    emitter.label("__rt_mb_strtoupper_iconv_map_loop_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 136]");                       // load the decode byte index
    emitter.instruction("cmp r8, QWORD PTR [rbp - 144]");                       // mapped every decoded scalar?
    emitter.instruction("jae __rt_mb_strtoupper_iconv_mapped_x86");             // encode once uppercase is finished
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // reload the decode buffer
    emitter.instruction("mov eax, DWORD PTR [r9 + r8]");                        // load the next UTF-32LE scalar
    emitter.instruction("add QWORD PTR [rbp - 136], 4");                        // consume four decode bytes
    emitter.instruction("cmp QWORD PTR [rbp - 168], 0");                        // has any payload scalar already been seen?
    emitter.instruction("jne __rt_mb_strtoupper_iconv_map_apply_x86");          // after the first payload scalar, keep every later BOM
    emitter.instruction("cmp eax, 0xFEFF");                                     // is this a leading UTF-32 BOM?
    emitter.instruction("je __rt_mb_strtoupper_iconv_map_loop_x86");            // skip a leading BOM
    emitter.label("__rt_mb_strtoupper_iconv_map_apply_x86");
    emitter.instruction("mov QWORD PTR [rbp - 168], 1");                        // later scalars are payload
    emitter.instruction("lea rdi, [rbp - 184]");                                // case-map destination is the 16-byte stack buffer
    emitter.instruction("call __rt_mb_case_upper");                             // expand the scalar through Unicode full uppercase
    emitter.instruction("mov r8, rax");                                         // preserve the uppercase scalar count
    emitter.instruction("shl rax, 2");                                          // each uppercase scalar occupies four UTF-32 bytes
    emitter.instruction("mov r9, QWORD PTR [rbp - 152]");                       // bytes already written to the uppercase buffer
    emitter.instruction("lea r10, [r9 + rax]");                                 // the bytes this mapping would occupy
    emitter.instruction("cmp r10, QWORD PTR [rbp - 160]");                      // does the mapping still fit?
    emitter.instruction("jbe __rt_mb_strtoupper_iconv_map_store_x86");          // copy when the current buffer is large enough
    emitter.instruction("mov QWORD PTR [rbp - 88], r8");                        // preserve the uppercase scalar count across grow
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // preserve the mapping byte count across grow
    emitter.instruction("mov rax, QWORD PTR [rbp - 160]");                      // current uppercase capacity
    emitter.instruction("shl rax, 1");                                          // double the uppercase capacity
    emitter.instruction("add rax, 16");                                         // plus a small extra so tiny buffers can grow
    emitter.instruction("mov QWORD PTR [rbp - 160], rax");                      // persist the new uppercase capacity
    emitter.instruction("call __rt_heap_alloc");                                // allocate the grown uppercase buffer
    emitter.instruction("mov QWORD PTR [rax - 8], 1");                          // stamp the grown uppercase temporary
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // old uppercase buffer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 152]");                      // bytes already written
    emitter.instruction("mov rdi, rax");                                        // grown uppercase buffer is the copy destination
    emitter.instruction("cld");                                                 // `rep movsb` requires a forward copy
    emitter.instruction("rep movsb");                                           // duplicate the uppercase prefix
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // persist the grown uppercase buffer
    emitter.instruction("mov rax, rsi");                                        // release the superseded uppercase buffer
    emitter.instruction("sub rax, QWORD PTR [rbp - 152]");                      // `rep movsb` advanced rsi past the copied prefix
    emitter.instruction("call __rt_heap_free");                                 // return the old temporary to the heap
    emitter.instruction("mov r8, QWORD PTR [rbp - 88]");                        // restore the uppercase scalar count
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // restore the mapping byte count
    emitter.label("__rt_mb_strtoupper_iconv_map_store_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // uppercase buffer
    emitter.instruction("add rdi, QWORD PTR [rbp - 152]");                      // write cursor
    emitter.instruction("lea rsi, [rbp - 184]");                                // case-map output
    emitter.instruction("mov rcx, rax");                                        // mapping byte count
    emitter.instruction("cld");                                                 // `rep movsb` requires a forward copy
    emitter.instruction("rep movsb");                                           // store the mapped UTF-32 scalars
    emitter.instruction("add QWORD PTR [rbp - 152], rax");                      // account for the mapped UTF-32 bytes
    emitter.instruction("jmp __rt_mb_strtoupper_iconv_map_loop_x86");           // continue mapping the remaining scalars

    emitter.label("__rt_mb_strtoupper_iconv_mapped_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 152], 0");                        // did mapping produce at least one scalar?
    emitter.instruction("jne __rt_mb_strtoupper_iconv_encode_x86");             // encode when mapping produced output
    emitter.instruction("call __rt_mb_strtoupper_iconv_free_temps_x86");        // release both UTF-32 temporaries
    emitter.instruction("jmp __rt_mb_strtoupper_empty_x86");                    // an empty mapping produces an empty string

    emitter.label("__rt_mb_strtoupper_iconv_encode_x86");
    emitter.instruction("lea rdi, [rbp - 256]");                                // encode destination encoding is the copied explicit name
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_utf32le_name");
    emitter.instruction("call iconv_open");                                     // create the UTF-32LE-to-encoding conversion descriptor
    emitter.instruction("cmp rax, -1");                                         // did the encoder reject the same encoding name?
    emitter.instruction("jne __rt_mb_strtoupper_iconv_encode_opened_x86");      // continue when the encoder opened
    emitter.instruction("call __rt_mb_strtoupper_iconv_free_temps_x86");        // release both UTF-32 temporaries before unwinding
    emitter.instruction("jmp __rt_mb_strtoupper_unknown_encoding_x86");         // treat a failed encoder open as an unknown encoding
    emitter.label("__rt_mb_strtoupper_iconv_encode_opened_x86");
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // preserve the encode descriptor
    emitter.instruction("mov rax, QWORD PTR [rbp - 152]");                      // reserve at least the uppercase UTF-32 byte count
    emitter.instruction("shl rax, 1");                                          // encodings can expand past UTF-32
    emitter.instruction("add rax, 16");                                         // plus a small extra so tiny results can encode
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // remember the reserved destination capacity
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the destination pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // encoded byte count starts at zero
    emitter.instruction("mov r8, QWORD PTR [rbp - 80]");                        // encode input starts at the uppercase UTF-32
    emitter.instruction("mov QWORD PTR [rbp - 104], r8");                       // initialize iconv's mutable input pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 152]");                       // encode still has the whole uppercase buffer
    emitter.instruction("mov QWORD PTR [rbp - 112], r8");                       // initialize iconv's mutable input-byte count

    emitter.label("__rt_mb_strtoupper_iconv_enc_loop_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 112], 0");                        // are any uppercase UTF-32 bytes still unencoded?
    emitter.instruction("je __rt_mb_strtoupper_iconv_encoded_x86");             // encode is finished when input is exhausted
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // destination base
    emitter.instruction("add r8, QWORD PTR [rbp - 48]");                        // current encode write cursor
    emitter.instruction("mov QWORD PTR [rbp - 120], r8");                       // initialize iconv's mutable output pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // destination capacity
    emitter.instruction("sub r8, QWORD PTR [rbp - 48]");                        // remaining destination bytes
    emitter.instruction("jz __rt_mb_strtoupper_iconv_enc_grow_x86");            // grow when the destination is already full
    emitter.instruction("mov QWORD PTR [rbp - 128], r8");                       // initialize iconv's mutable output-byte count
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // iconv argument 0 is the encode descriptor
    emitter.instruction("lea rsi, [rbp - 104]");                                // iconv argument 1 is `&input_ptr`
    emitter.instruction("lea rdx, [rbp - 112]");                                // iconv argument 2 is `&input_bytes_left`
    emitter.instruction("lea rcx, [rbp - 120]");                                // iconv argument 3 is `&output_ptr`
    emitter.instruction("lea r8, [rbp - 128]");                                 // iconv argument 4 is `&output_bytes_left`
    emitter.instruction("call iconv");                                          // encode as many complete characters as fit
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the destination capacity
    emitter.instruction("sub r8, QWORD PTR [rbp - 128]");                       // written = capacity - unused
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // persist the accumulated encoded byte count
    emitter.instruction("cmp rax, -1");                                         // did iconv report an error condition?
    emitter.instruction("jne __rt_mb_strtoupper_iconv_enc_loop_x86");           // successful progress continues
    emitter.instruction("cmp QWORD PTR [rbp - 128], 0");                        // did iconv merely fill the destination?
    emitter.instruction("je __rt_mb_strtoupper_iconv_enc_grow_x86");            // a full destination needs a larger reservation
    emitter.instruction("call __errno_location");                               // fetch the Linux thread-local errno written by iconv
    emitter.instruction("cmp DWORD PTR [rax], 22");                             // EINVAL means a truncated final sequence
    emitter.instruction("je __rt_mb_strtoupper_iconv_encoded_x86");             // stop at a truncated suffix
    emitter.instruction("cmp QWORD PTR [rbp - 112], 0");                        // are bytes still present at the malformed sequence?
    emitter.instruction("je __rt_mb_strtoupper_iconv_encoded_x86");             // defensive completion
    emitter.instruction("add QWORD PTR [rbp - 104], 1");                        // skip one malformed input byte
    emitter.instruction("sub QWORD PTR [rbp - 112], 1");                        // remove the malformed byte
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // reload the encode descriptor
    emitter.instruction("xor rsi, rsi");                                        // reset iconv shift state
    emitter.instruction("xor rdx, rdx");                                        // no input participates in the reset
    emitter.instruction("xor rcx, rcx");                                        // no output participates in the reset
    emitter.instruction("xor r8, r8");                                          // no output count participates in the reset
    emitter.instruction("call iconv");                                          // reset stateful encoders after skipping one malformed byte
    emitter.instruction("jmp __rt_mb_strtoupper_iconv_enc_loop_x86");           // continue encoding

    emitter.label("__rt_mb_strtoupper_iconv_enc_grow_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // current destination buffer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // preserve the bytes already encoded
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // current destination capacity
    emitter.instruction("shl rsi, 1");                                          // double the destination capacity
    emitter.instruction("add rsi, 16");                                         // plus a small extra so tiny results can grow
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");                       // persist the new capacity before the grow call
    emitter.instruction("call __rt_concat_grow");                               // replace the destination with a larger owned buffer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // persist the grown destination pointer
    emitter.instruction("jmp __rt_mb_strtoupper_iconv_enc_loop_x86");           // retry the conversion with the larger destination

    emitter.label("__rt_mb_strtoupper_iconv_encoded_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 96]");                       // close the encode descriptor
    emitter.instruction("call iconv_close");                                    // release the UTF-32LE-to-encoding descriptor
    emitter.instruction("call __rt_mb_strtoupper_iconv_free_temps_x86");        // release both UTF-32 temporaries
    emitter.instruction("jmp __rt_mb_strtoupper_finish_x86");                   // publish the encoded uppercase string

    emitter.label("__rt_mb_strtoupper_iconv_free_temps_x86");
    emitter.instruction("sub rsp, 8");                                          // frameless helper: pad so nested heap_free calls are SysV-aligned
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // decode buffer
    emitter.instruction("call __rt_heap_free");                                 // release the decode temporary
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // uppercase buffer
    emitter.instruction("test rax, rax");                                       // did mapping allocate an uppercase temporary?
    emitter.instruction("jz __rt_mb_strtoupper_iconv_free_done_x86");           // skip when mapping never allocated
    emitter.instruction("call __rt_heap_free");                                 // release the uppercase temporary
    emitter.label("__rt_mb_strtoupper_iconv_free_done_x86");
    emitter.instruction("add rsp, 8");                                          // drop the SysV alignment pad
    emitter.instruction("ret");                                                 // return to the iconv path
}

/// Emits destination-capacity growth and UTF-8 append helpers used by the walker.
fn emit_helpers_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtoupper_ensure_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // load the number of destination bytes already written
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // load the reserved destination capacity
    emitter.instruction("lea r10, [r8 + 12]");                                  // the next scalar can emit up to twelve UTF-8 bytes
    emitter.instruction("cmp r10, r9");                                         // does the reservation still have twelve free bytes?
    emitter.instruction("jbe __rt_mb_strtoupper_ensure_done_x86");              // keep the current reservation when it still fits
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // current destination buffer
    emitter.instruction("mov rdi, r8");                                         // preserve the bytes already written
    emitter.instruction("mov rsi, r9");                                         // current destination capacity
    emitter.instruction("shl rsi, 1");                                          // double the destination capacity
    emitter.instruction("add rsi, 16");                                         // plus a small extra so tiny strings can grow
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");                       // persist the new capacity before the grow call
    emitter.instruction("sub rsp, 8");                                          // frameless helper: pad so concat_grow is SysV-aligned
    emitter.instruction("call __rt_concat_grow");                               // replace the destination with a larger owned buffer
    emitter.instruction("add rsp, 8");                                          // drop the SysV alignment pad
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // persist the grown destination pointer
    emitter.label("__rt_mb_strtoupper_ensure_done_x86");
    emitter.instruction("ret");                                                 // return to the UTF-8 walker

    emitter.label("__rt_mb_strtoupper_put_utf8_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // load the destination pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // load the number of bytes already written
    emitter.instruction("cmp eax, 0x80");                                       // one-byte UTF-8?
    emitter.instruction("jae __rt_mb_strtoupper_put_utf8_2_x86");               // encode a longer sequence
    emitter.instruction("mov BYTE PTR [r8 + r9], al");                          // store the ASCII byte
    emitter.instruction("inc QWORD PTR [rbp - 48]");                            // one byte was written
    emitter.instruction("ret");                                                 // return after the one-byte encode
    emitter.label("__rt_mb_strtoupper_put_utf8_2_x86");
    emitter.instruction("cmp eax, 0x800");                                      // two-byte UTF-8?
    emitter.instruction("jae __rt_mb_strtoupper_put_utf8_3_x86");               // encode a three- or four-byte sequence
    emitter.instruction("mov ecx, eax");                                        // copy the scalar so the leader can be formed
    emitter.instruction("shr ecx, 6");                                          // high five bits
    emitter.instruction("or ecx, 0xC0");                                        // two-byte leader
    emitter.instruction("mov BYTE PTR [r8 + r9], cl");                          // store the two-byte leader
    emitter.instruction("mov ecx, eax");                                        // reload the scalar for the continuation
    emitter.instruction("and ecx, 0x3F");                                       // low six bits
    emitter.instruction("or ecx, 0x80");                                        // continuation shape
    emitter.instruction("mov BYTE PTR [r8 + r9 + 1], cl");                      // store the continuation
    emitter.instruction("add QWORD PTR [rbp - 48], 2");                         // two bytes were written
    emitter.instruction("ret");                                                 // return after the two-byte encode
    emitter.label("__rt_mb_strtoupper_put_utf8_3_x86");
    emitter.instruction("cmp eax, 0x10000");                                    // three-byte UTF-8?
    emitter.instruction("jae __rt_mb_strtoupper_put_utf8_4_x86");               // encode a four-byte sequence
    emitter.instruction("mov ecx, eax");                                        // copy the scalar so the leader can be formed
    emitter.instruction("shr ecx, 12");                                         // high four bits
    emitter.instruction("or ecx, 0xE0");                                        // three-byte leader
    emitter.instruction("mov BYTE PTR [r8 + r9], cl");                          // store the three-byte leader
    emitter.instruction("mov ecx, eax");                                        // reload the scalar for the first continuation
    emitter.instruction("shr ecx, 6");                                          // middle six bits
    emitter.instruction("and ecx, 0x3F");                                       // isolate the continuation payload
    emitter.instruction("or ecx, 0x80");                                        // continuation shape
    emitter.instruction("mov BYTE PTR [r8 + r9 + 1], cl");                      // store the first continuation
    emitter.instruction("mov ecx, eax");                                        // reload the scalar for the final continuation
    emitter.instruction("and ecx, 0x3F");                                       // low six bits
    emitter.instruction("or ecx, 0x80");                                        // continuation shape
    emitter.instruction("mov BYTE PTR [r8 + r9 + 2], cl");                      // store the final continuation
    emitter.instruction("add QWORD PTR [rbp - 48], 3");                         // three bytes were written
    emitter.instruction("ret");                                                 // return after the three-byte encode
    emitter.label("__rt_mb_strtoupper_put_utf8_4_x86");
    emitter.instruction("mov ecx, eax");                                        // copy the scalar so the leader can be formed
    emitter.instruction("shr ecx, 18");                                         // high three bits
    emitter.instruction("or ecx, 0xF0");                                        // four-byte leader
    emitter.instruction("mov BYTE PTR [r8 + r9], cl");                          // store the four-byte leader
    emitter.instruction("mov ecx, eax");                                        // reload the scalar for the first continuation
    emitter.instruction("shr ecx, 12");                                         // next six bits
    emitter.instruction("and ecx, 0x3F");                                       // isolate the continuation payload
    emitter.instruction("or ecx, 0x80");                                        // continuation shape
    emitter.instruction("mov BYTE PTR [r8 + r9 + 1], cl");                      // store the first continuation
    emitter.instruction("mov ecx, eax");                                        // reload the scalar for the second continuation
    emitter.instruction("shr ecx, 6");                                          // next six bits
    emitter.instruction("and ecx, 0x3F");                                       // isolate the continuation payload
    emitter.instruction("or ecx, 0x80");                                        // continuation shape
    emitter.instruction("mov BYTE PTR [r8 + r9 + 2], cl");                      // store the second continuation
    emitter.instruction("mov ecx, eax");                                        // reload the scalar for the final continuation
    emitter.instruction("and ecx, 0x3F");                                       // low six bits
    emitter.instruction("or ecx, 0x80");                                        // continuation shape
    emitter.instruction("mov BYTE PTR [r8 + r9 + 3], cl");                      // store the final continuation
    emitter.instruction("add QWORD PTR [rbp - 48], 4");                         // four bytes were written
    emitter.instruction("ret");                                                 // return after the four-byte encode
}
