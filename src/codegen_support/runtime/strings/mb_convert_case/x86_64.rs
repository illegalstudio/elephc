//! Purpose:
//! Linux x86_64 implementation of `__rt_mb_convert_case`.
//!
//! Called from:
//! - `super::emit_mb_convert_case()`.
//!
//! Key details:
//! - Arguments: `rax`/`rdx` string, `rdi` mode, `r8`/`r9` optional encoding.
//! - Result is reserved through `__rt_concat_reserve` and published with `__rt_concat_publish`.
//! - `map_len` at `[rbp-232]` and the emit index at `[rbp-228]` are packed 32-bit
//!   slots so a 64-bit store cannot overlap the neighboring word.
//! - Apply and sigma-ahead pad `rsp` by 8 so nested `call` sites stay System V aligned.

use super::{MAX_ENCODING_NAME_LEN, RESERVE_MULTIPLIER};
use crate::codegen_support::{
    abi,
    emit::Emitter,
    runtime::{arrays::value_error, data::MB_CONVERT_CASE_BAD_ENCODING_MSG, data::MB_CONVERT_CASE_BAD_MODE_MSG},
};

/// Emits the x86_64 System V implementation.
pub(super) fn emit_mb_convert_case_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_convert_case (PHP 8.5 Unicode case conversion) ---");
    emitter.label_global("__rt_mb_convert_case");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across libc and helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned helper frame
    emitter.instruction("sub rsp, 320");                                        // reserve convert state, iconv pointers, and the encoding-name buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source string length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the MB_CASE_* mode integer
    emitter.instruction("mov QWORD PTR [rbp - 136], r8");                       // save the optional encoding pointer
    emitter.instruction("mov QWORD PTR [rbp - 144], r9");                       // save the optional encoding length
    emitter.instruction("mov QWORD PTR [rbp - 220], 0");                        // iconv reverse-encode is off unless a non-UTF-8 name was used
    emitter.instruction(&format!(
        "cmp rdi, {}",
        crate::types::string_constants::MB_CASE_MODE_MAX
    ));                                                                         // is the mode one of PHP's MB_CASE_* constants?
    emitter.instruction("ja __rt_mb_cc_bad_mode_x86");                          // reject modes outside 0..=7 with ValueError
    emitter.instruction("test r8, r8");                                         // omitted/null encoding is a null pointer
    emitter.instruction("jz __rt_mb_cc_utf8_x86");                              // default encoding is UTF-8
    emitter.instruction(&format!("cmp r9, {}", MAX_ENCODING_NAME_LEN));         // does the encoding name fit the stack C-string buffer?
    emitter.instruction("ja __rt_mb_cc_bad_encoding_x86");                      // reject names longer than every PHP-supported alias
    emitter.instruction("lea rdi, [rbp - 320]");                                // destination is the 64-byte encoding-name buffer
    emitter.instruction("xor rcx, rcx");                                        // copied-byte index starts at zero
    emitter.label("__rt_mb_cc_enc_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied the whole explicit encoding name?
    emitter.instruction("jae __rt_mb_cc_enc_copied_x86");                       // terminate the C string once every byte is copied
    emitter.instruction("mov r10b, BYTE PTR [r8 + rcx]");                       // load one encoding-name byte from the PHP string
    emitter.instruction("mov BYTE PTR [rdi + rcx], r10b");                      // append the byte to the stack C string
    emitter.instruction("inc rcx");                                             // advance the encoding-name byte index
    emitter.instruction("jmp __rt_mb_cc_enc_copy_x86");                         // continue copying the remaining encoding-name bytes
    emitter.label("__rt_mb_cc_enc_copied_x86");
    emitter.instruction("mov BYTE PTR [rdi + r9], 0");                          // NUL-terminate the explicit encoding name
    emitter.instruction("lea rdi, [rbp - 320]");                                // first strcasecmp argument is the copied encoding name
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_utf8_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with UTF-8 case-insensitively
    emitter.instruction("test eax, eax");                                       // did the encoding match UTF-8?
    emitter.instruction("jz __rt_mb_cc_utf8_x86");                              // UTF-8 uses the Unicode converter
    emitter.instruction("lea rdi, [rbp - 320]");                                // reload the copied encoding name after strcasecmp
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_utf8_alias");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with PHP's UTF8 alias
    emitter.instruction("test eax, eax");                                       // did the encoding match UTF8?
    emitter.instruction("jz __rt_mb_cc_utf8_x86");                              // the UTF8 alias uses the same converter
    emitter.instruction("lea rdi, [rbp - 320]");                                // reload the copied encoding name for the byte encodings
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_8bit_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with 8bit
    emitter.instruction("test eax, eax");                                       // did the encoding match 8bit?
    emitter.instruction("jz __rt_mb_cc_latin1_x86");                            // 8bit treats each byte as U+00xx
    emitter.instruction("lea rdi, [rbp - 320]");                                // reload the copied encoding name for the binary alias
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_binary_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with binary
    emitter.instruction("test eax, eax");                                       // did the encoding match binary?
    emitter.instruction("jz __rt_mb_cc_latin1_x86");                            // binary is PHP's alias for 8bit
    emitter.instruction("lea rdi, [rbp - 320]");                                // reload the copied encoding name for 7bit
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_7bit_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with 7bit
    emitter.instruction("test eax, eax");                                       // did the encoding match 7bit?
    emitter.instruction("jz __rt_mb_cc_latin1_x86");                            // 7bit preserves the one-byte identity encoding
    emitter.instruction("jmp __rt_mb_cc_iconv_x86");                            // remaining names decode through libc iconv

    emitter.label("__rt_mb_cc_utf8_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // UTF-8 mode decodes validated Unicode scalars
    emitter.instruction("jmp __rt_mb_cc_convert_x86");                          // convert the saved source string

    emitter.label("__rt_mb_cc_latin1_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], 1");                         // 8bit/binary/7bit treat every byte as U+00xx

    emitter.label("__rt_mb_cc_convert_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the source length before computing the reservation
    emitter.instruction(&format!("imul rax, {}", RESERVE_MULTIPLIER));          // reserve four output bytes per input byte for full mappings
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the converted string
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // remember the destination start pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // destination cursor starts at the reserved buffer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // source byte offset starts at zero
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // title_mode starts false
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // previous non-ignorable cased flag starts false

    emitter.label("__rt_mb_cc_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the current source offset
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // have we consumed every source byte?
    emitter.instruction("jae __rt_mb_cc_done_x86");                             // publish once the source is exhausted
    emitter.instruction("call __rt_mb_cc_decode_x86");                          // decode the next UTF-8 or 8-bit unit
    emitter.instruction("cmp QWORD PTR [rbp - 88], 1");                         // was the unit a malformed/raw byte group?
    emitter.instruction("je __rt_mb_cc_raw_x86");                               // copy malformed bytes through unchanged
    emitter.instruction("call __rt_mb_cc_apply_x86");                           // map the scalar and emit UTF-8 or Latin-1 bytes
    emitter.instruction("jmp __rt_mb_cc_loop_x86");                             // continue with the next source unit

    emitter.label("__rt_mb_cc_raw_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // load the source pointer
    emitter.instruction("add rsi, QWORD PTR [rbp - 48]");                       // point at the raw byte group
    emitter.instruction("mov rcx, QWORD PTR [rbp - 96]");                       // load the raw group length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // load the destination cursor
    emitter.instruction("rep movsb");                                           // copy the malformed group through unchanged
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // store the advanced destination cursor
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the source offset
    emitter.instruction("add rax, QWORD PTR [rbp - 96]");                       // consume the raw group
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // persist the advanced source offset
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // raw units reset title_mode
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // raw units are non-cased and non-ignorable
    emitter.instruction("jmp __rt_mb_cc_loop_x86");                             // continue with the next source unit

    emitter.label("__rt_mb_cc_done_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 220], 0");                        // does the result still need encoding back from UTF-8?
    emitter.instruction("jne __rt_mb_cc_iconv_from_utf8_x86");                  // reverse-encode iconv results into the original encoding
    emitter.label("__rt_mb_cc_publish_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // result pointer is the reserved destination start
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // compute the written length from the cursor
    emitter.instruction("sub rdx, rax");                                        // rdx = dest_cur - dest_start
    emitter.instruction("call __rt_concat_publish");                            // publish scratch-backed results
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the converted string

    emit_iconv_x86_64(emitter);
    emit_decode_x86_64(emitter);
    emit_apply_x86_64(emitter);
    emit_table_helpers_x86_64(emitter);
    emit_encode_x86_64(emitter);

    emitter.label("__rt_mb_cc_bad_mode_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before throwing
    value_error::emit_throw_value_error_x86_64(
        emitter,
        "_mb_convert_case_bad_mode_msg",
        MB_CONVERT_CASE_BAD_MODE_MSG.len(),
    );

    emitter.label("__rt_mb_cc_bad_encoding_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before throwing
    value_error::emit_throw_value_error_x86_64(
        emitter,
        "_mb_convert_case_bad_encoding_msg",
        MB_CONVERT_CASE_BAD_ENCODING_MSG.len(),
    );
}

/// Emits iconv-backed conversion for encodings other than UTF-8 and 8bit aliases.
fn emit_iconv_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_cc_iconv_x86");
    abi::emit_symbol_address(emitter, "rdi", "_mb_strlen_utf8_name");
    emitter.instruction("lea rsi, [rbp - 320]");                                // iconv source encoding is the copied explicit name
    emitter.instruction("call iconv_open");                                     // open a decoder from the requested encoding into UTF-8
    emitter.instruction("cmp rax, -1");                                         // did iconv_open return the failure sentinel?
    emitter.instruction("je __rt_mb_cc_bad_encoding_x86");                      // unknown encoding names raise PHP's ValueError
    emitter.instruction("mov QWORD PTR [rbp - 168], rax");                      // preserve the to-UTF-8 iconv descriptor
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // load the source length
    emitter.instruction("imul rax, 4");                                         // reserve four UTF-8 bytes per input byte
    emitter.instruction("add rax, 16");                                         // keep a small slack buffer for the decoder
    emitter.instruction("call __rt_heap_alloc");                                // allocate a temporary UTF-8 decode buffer
    emitter.instruction("mov QWORD PTR [rbp - 184], rax");                      // save the UTF-8 decode buffer pointer
    emitter.instruction("lea rax, [rbp - 8]");                                  // iconv inbuf is a pointer to the source pointer slot
    emitter.instruction("lea rdx, [rbp - 16]");                                 // iconv inbytesleft is the source length slot
    emitter.instruction("mov rdi, QWORD PTR [rbp - 168]");                      // reload the decoder descriptor
    emitter.instruction("mov rsi, rax");                                        // pass the mutable input pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 184]");                      // load the UTF-8 output buffer
    emitter.instruction("mov QWORD PTR [rbp - 192], rax");                      // mutable outbuf pointer starts at the UTF-8 buffer
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the original source length for the output capacity
    emitter.instruction("imul rax, 4");                                         // output capacity matches the allocated UTF-8 buffer
    emitter.instruction("add rax, 16");                                         // include the slack bytes
    emitter.instruction("mov QWORD PTR [rbp - 176], rax");                      // outbytesleft starts at the allocated capacity
    emitter.label("__rt_mb_cc_iconv_to_utf8_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // any input bytes remaining?
    emitter.instruction("je __rt_mb_cc_iconv_to_utf8_done_x86");                // close the decoder after the input is consumed
    emitter.instruction("mov rdi, QWORD PTR [rbp - 168]");                      // reload the decoder descriptor
    emitter.instruction("lea rsi, [rbp - 8]");                                  // inbuf pointer slot
    emitter.instruction("lea rdx, [rbp - 16]");                                 // inbytesleft slot
    emitter.instruction("lea rcx, [rbp - 192]");                                // outbuf pointer slot
    emitter.instruction("lea r8, [rbp - 176]");                                 // outbytesleft slot
    emitter.instruction("call iconv");                                          // decode the next UTF-8 chunk
    emitter.instruction("cmp rax, -1");                                         // did iconv report an error?
    emitter.instruction("jne __rt_mb_cc_iconv_to_utf8_x86");                    // successful progress continues until input is exhausted
    emitter.instruction("call __errno_location");                               // load errno after an iconv failure
    emitter.instruction("mov eax, DWORD PTR [rax]");                            // read the errno value
    emitter.instruction("cmp eax, 7");                                          // E2BIG?
    emitter.instruction("je __rt_mb_cc_iconv_to_utf8_x86");                     // a full output buffer only requires another iteration
    emitter.instruction("cmp eax, 22");                                         // EINVAL (Linux)?
    emitter.instruction("je __rt_mb_cc_iconv_to_utf8_done_x86");                // a truncated suffix is dropped like Magician's iconv path
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // defensive completion if the last byte was consumed
    emitter.instruction("je __rt_mb_cc_iconv_to_utf8_done_x86");                // close the decoder
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // load the current input pointer
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // copy the malformed byte into the UTF-8 buffer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 192]");                      // load the current UTF-8 output pointer
    emitter.instruction("mov BYTE PTR [rdx], cl");                              // store the copied malformed byte
    emitter.instruction("inc QWORD PTR [rbp - 192]");                           // advance the UTF-8 output pointer
    emitter.instruction("dec QWORD PTR [rbp - 176]");                           // consume one output byte of capacity
    emitter.instruction("inc QWORD PTR [rbp - 8]");                             // skip the malformed input byte
    emitter.instruction("dec QWORD PTR [rbp - 16]");                            // consume the malformed input byte
    emitter.instruction("jmp __rt_mb_cc_iconv_to_utf8_x86");                    // continue decoding after the malformed byte

    emitter.label("__rt_mb_cc_iconv_to_utf8_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 168]");                      // reload the decoder descriptor
    emitter.instruction("call iconv_close");                                    // release the to-UTF-8 descriptor
    emitter.instruction("mov rax, QWORD PTR [rbp - 192]");                      // load the UTF-8 write cursor
    emitter.instruction("sub rax, QWORD PTR [rbp - 184]");                      // compute the decoded UTF-8 length
    emitter.instruction("mov rdx, rax");                                        // keep the UTF-8 length
    emitter.instruction("mov rax, QWORD PTR [rbp - 184]");                      // convert the decoded UTF-8 buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // source pointer is the temporary UTF-8 buffer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // source length is the decoded UTF-8 length
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // decoded iconv bytes are UTF-8
    emitter.instruction("mov QWORD PTR [rbp - 220], 1");                        // convert back to the original encoding after UTF-8 mapping
    emitter.instruction("jmp __rt_mb_cc_convert_x86");                          // reuse the UTF-8 converter for the iconv decode buffer

    emitter.label("__rt_mb_cc_iconv_from_utf8_x86");
    emitter.instruction("lea rdi, [rbp - 320]");                                // original encoding name
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_utf8_name");
    emitter.instruction("call iconv_open");                                     // open an encoder from UTF-8 back to the original encoding
    emitter.instruction("cmp rax, -1");                                         // did the reverse conversion reject the encoding?
    emitter.instruction("je __rt_mb_cc_bad_encoding_x86");                      // treat a failed reverse open as an unknown encoding
    emitter.instruction("mov QWORD PTR [rbp - 168], rax");                      // preserve the from-UTF-8 iconv descriptor
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // placeholder overwritten below
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // converted UTF-8 pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // iconv input is the converted UTF-8 string
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // converted UTF-8 cursor
    emitter.instruction("sub rax, QWORD PTR [rbp - 32]");                       // converted UTF-8 length
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // iconv input length
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the converted length
    emitter.instruction("imul rax, 4");                                         // reserve four bytes per converted UTF-8 byte
    emitter.instruction("add rax, 16");                                         // keep slack for the encoder
    emitter.instruction("call __rt_heap_alloc");                                // allocate the reverse-encoded result
    emitter.instruction("mov QWORD PTR [rbp - 184], rax");                      // save the reverse-encoded buffer
    emitter.instruction("mov QWORD PTR [rbp - 192], rax");                      // mutable outbuf starts at that buffer
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the converted UTF-8 length
    emitter.instruction("imul rax, 4");                                         // output capacity
    emitter.instruction("add rax, 16");                                         // include slack
    emitter.instruction("mov QWORD PTR [rbp - 176], rax");                      // outbytesleft
    emitter.label("__rt_mb_cc_iconv_from_loop_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // any converted UTF-8 bytes remaining?
    emitter.instruction("je __rt_mb_cc_iconv_from_done_x86");                   // finish after the converted UTF-8 is consumed
    emitter.instruction("mov rdi, QWORD PTR [rbp - 168]");                      // reload the encoder descriptor
    emitter.instruction("lea rsi, [rbp - 8]");                                  // inbuf pointer slot
    emitter.instruction("lea rdx, [rbp - 16]");                                 // inbytesleft slot
    emitter.instruction("lea rcx, [rbp - 192]");                                // outbuf pointer slot
    emitter.instruction("lea r8, [rbp - 176]");                                 // outbytesleft slot
    emitter.instruction("call iconv");                                          // encode the next chunk
    emitter.instruction("cmp rax, -1");                                         // did iconv report an error?
    emitter.instruction("jne __rt_mb_cc_iconv_from_loop_x86");                  // successful progress continues
    emitter.instruction("jmp __rt_mb_cc_iconv_from_done_x86");                  // stop on encoder errors and publish what was written

    emitter.label("__rt_mb_cc_iconv_from_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 168]");                      // reload the encoder descriptor
    emitter.instruction("call iconv_close");                                    // release the from-UTF-8 descriptor
    emitter.instruction("mov rax, QWORD PTR [rbp - 184]");                      // reverse-encoded pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 192]");                      // reverse-encoded cursor
    emitter.instruction("sub rdx, rax");                                        // reverse-encoded length
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // publish this buffer as the result pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // cursor for publish length math
    emitter.instruction("add QWORD PTR [rbp - 40], rdx");                       // dest_cur = dest_start + length
    emitter.instruction("mov QWORD PTR [rbp - 220], 0");                        // prevent a second reverse-encode pass
    emitter.instruction("jmp __rt_mb_cc_publish_x86");                          // publish the reverse-encoded string
}

/// Emits UTF-8 / 8-bit decode of the unit at the current source offset.
fn emit_decode_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_cc_decode_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // load the source pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // load the current source offset
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // load the source length
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // is this the UTF-8 decoder?
    emitter.instruction("je __rt_mb_cc_decode_utf8_x86");                       // UTF-8 validates multi-byte sequences
    emitter.instruction("movzx eax, BYTE PTR [rsi + rcx]");                     // 8bit reads the next byte as U+00xx
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // store the Latin-1 code point
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // 8bit units are always scalars
    emitter.instruction("mov QWORD PTR [rbp - 96], 1");                         // each 8bit unit consumes one byte
    emitter.instruction("ret");                                                 // return the decoded 8-bit unit

    emitter.label("__rt_mb_cc_decode_utf8_x86");
    emitter.instruction("movzx eax, BYTE PTR [rsi + rcx]");                     // load the next possible UTF-8 leading byte
    emitter.instruction("cmp eax, 0x80");                                       // ASCII bytes are complete one-byte characters
    emitter.instruction("jb __rt_mb_cc_decode_ascii_x86");                      // consume one ASCII scalar
    emitter.instruction("cmp eax, 0xC2");                                       // C0/C1 and continuation bytes are malformed leaders
    emitter.instruction("jb __rt_mb_cc_decode_raw1_x86");                       // substitute one malformed byte
    emitter.instruction("cmp eax, 0xE0");                                       // C2-DF begin two-byte characters
    emitter.instruction("jb __rt_mb_cc_decode_two_x86");                        // validate a two-byte character
    emitter.instruction("cmp eax, 0xF0");                                       // E0-EF begin three-byte characters
    emitter.instruction("jb __rt_mb_cc_decode_three_x86");                      // validate a three-byte character
    emitter.instruction("cmp eax, 0xF5");                                       // F0-F4 begin four-byte characters
    emitter.instruction("jb __rt_mb_cc_decode_four_x86");                       // validate a four-byte character
    emitter.label("__rt_mb_cc_decode_raw1_x86");
    emitter.instruction("mov QWORD PTR [rbp - 88], 1");                         // mark the unit as a raw malformed group
    emitter.instruction("mov QWORD PTR [rbp - 96], 1");                         // consume the malformed leader alone
    emitter.instruction("ret");                                                 // return the raw unit

    emitter.label("__rt_mb_cc_decode_ascii_x86");
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // store the ASCII code point
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // ASCII units are scalars
    emitter.instruction("mov QWORD PTR [rbp - 96], 1");                         // ASCII consumes one byte
    emitter.instruction("ret");                                                 // return the ASCII scalar

    emitter.label("__rt_mb_cc_decode_two_x86");
    emitter.instruction("mov r8, rdx");                                         // copy the source length
    emitter.instruction("sub r8, rcx");                                         // remaining bytes including the leader
    emitter.instruction("cmp r8, 2");                                           // is a complete two-byte sequence present?
    emitter.instruction("jb __rt_mb_cc_decode_trunc_x86");                      // group a truncated valid prefix as one raw unit
    emitter.instruction("movzx r9d, BYTE PTR [rsi + rcx + 1]");                 // load the continuation byte
    emitter.instruction("mov r10d, r9d");                                       // copy the continuation before masking
    emitter.instruction("and r10d, 0xC0");                                      // isolate the continuation prefix
    emitter.instruction("cmp r10d, 0x80");                                      // is it a well-formed continuation?
    emitter.instruction("jne __rt_mb_cc_decode_raw1_x86");                      // malformed continuation leaves the leader substituted alone
    emitter.instruction("and eax, 0x1F");                                       // keep the two-byte payload bits
    emitter.instruction("shl eax, 6");                                          // shift the leader payload into place
    emitter.instruction("and r9d, 0x3F");                                       // keep the continuation payload
    emitter.instruction("or eax, r9d");                                         // assemble the scalar
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // store the two-byte scalar
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // mark the unit as a scalar
    emitter.instruction("mov QWORD PTR [rbp - 96], 2");                         // consume both bytes
    emitter.instruction("ret");                                                 // return the two-byte scalar

    emitter.label("__rt_mb_cc_decode_three_x86");
    emitter.instruction("mov r8, rdx");                                         // copy the source length
    emitter.instruction("sub r8, rcx");                                         // remaining bytes including the leader
    emitter.instruction("cmp r8, 3");                                           // is a complete three-byte sequence present?
    emitter.instruction("jb __rt_mb_cc_decode_trunc_x86");                      // group a truncated valid prefix as one raw unit
    emitter.instruction("movzx r9d, BYTE PTR [rsi + rcx + 1]");                 // load the first continuation
    emitter.instruction("movzx r11d, BYTE PTR [rsi + rcx + 2]");                // load the second continuation
    emitter.instruction("mov r10d, r9d");                                       // copy the first continuation before masking
    emitter.instruction("and r10d, 0xC0");                                      // isolate the continuation prefix
    emitter.instruction("cmp r10d, 0x80");                                      // is the first continuation well-formed?
    emitter.instruction("jne __rt_mb_cc_decode_raw1_x86");                      // malformed continuation leaves the leader substituted alone
    emitter.instruction("mov r10d, r11d");                                      // copy the second continuation
    emitter.instruction("and r10d, 0xC0");                                      // isolate the continuation prefix
    emitter.instruction("cmp r10d, 0x80");                                      // is the second continuation well-formed?
    emitter.instruction("jne __rt_mb_cc_decode_raw1_x86");                      // malformed tail leaves the leader substituted alone
    emitter.instruction("cmp eax, 0xE0");                                       // E0 needs a lower-bound check
    emitter.instruction("jne __rt_mb_cc_decode_three_ed_x86");                  // skip the E0 bound for other leaders
    emitter.instruction("cmp r9d, 0xA0");                                       // reject overlong three-byte sequences
    emitter.instruction("jb __rt_mb_cc_decode_raw1_x86");                       // overlong encodings are malformed
    emitter.label("__rt_mb_cc_decode_three_ed_x86");
    emitter.instruction("cmp eax, 0xED");                                       // ED needs a surrogate bound
    emitter.instruction("jne __rt_mb_cc_decode_three_ok_x86");                  // skip the surrogate bound for other leaders
    emitter.instruction("cmp r9d, 0xA0");                                       // reject UTF-8 encodings of surrogate code points
    emitter.instruction("jae __rt_mb_cc_decode_raw1_x86");                      // surrogate encodings are malformed
    emitter.label("__rt_mb_cc_decode_three_ok_x86");
    emitter.instruction("and eax, 0x0F");                                       // keep the three-byte payload bits
    emitter.instruction("shl eax, 12");                                         // shift the leader payload into place
    emitter.instruction("and r9d, 0x3F");                                       // keep the first continuation payload
    emitter.instruction("shl r9d, 6");                                          // shift the first continuation into place
    emitter.instruction("and r11d, 0x3F");                                      // keep the second continuation payload
    emitter.instruction("or eax, r9d");                                         // assemble the high bits
    emitter.instruction("or eax, r11d");                                        // assemble the scalar
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // store the three-byte scalar
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // mark the unit as a scalar
    emitter.instruction("mov QWORD PTR [rbp - 96], 3");                         // consume all three bytes
    emitter.instruction("ret");                                                 // return the three-byte scalar

    emitter.label("__rt_mb_cc_decode_four_x86");
    emitter.instruction("mov r8, rdx");                                         // copy the source length
    emitter.instruction("sub r8, rcx");                                         // remaining bytes including the leader
    emitter.instruction("cmp r8, 4");                                           // is a complete four-byte sequence present?
    emitter.instruction("jb __rt_mb_cc_decode_trunc_x86");                      // group a truncated valid prefix as one raw unit
    emitter.instruction("movzx r9d, BYTE PTR [rsi + rcx + 1]");                 // load the first continuation
    emitter.instruction("mov r10d, r9d");                                       // copy the first continuation
    emitter.instruction("and r10d, 0xC0");                                      // isolate the continuation prefix
    emitter.instruction("cmp r10d, 0x80");                                      // is the first continuation well-formed?
    emitter.instruction("jne __rt_mb_cc_decode_raw1_x86");                      // malformed continuation leaves the leader substituted alone
    emitter.instruction("cmp eax, 0xF0");                                       // F0 needs a lower-bound check
    emitter.instruction("jne __rt_mb_cc_decode_four_f4_x86");                   // skip the F0 bound for other leaders
    emitter.instruction("cmp r9d, 0x90");                                       // reject overlong four-byte sequences
    emitter.instruction("jb __rt_mb_cc_decode_raw1_x86");                       // overlong encodings are malformed
    emitter.label("__rt_mb_cc_decode_four_f4_x86");
    emitter.instruction("cmp eax, 0xF4");                                       // F4 needs an upper bound
    emitter.instruction("jne __rt_mb_cc_decode_four_cont_x86");                  // F0-F3 continue
    emitter.instruction("cmp r9d, 0x90");                                       // reject out-of-range four-byte sequences
    emitter.instruction("jae __rt_mb_cc_decode_raw1_x86");                      // code points above U+10FFFF are malformed
    emitter.label("__rt_mb_cc_decode_four_cont_x86");
    emitter.instruction("movzx r11d, BYTE PTR [rsi + rcx + 2]");                // load the second continuation
    emitter.instruction("mov r10d, r11d");                                      // copy the second continuation
    emitter.instruction("and r10d, 0xC0");                                      // isolate the continuation prefix
    emitter.instruction("cmp r10d, 0x80");                                      // is the second continuation well-formed?
    emitter.instruction("jne __rt_mb_cc_decode_raw1_x86");                      // malformed continuation leaves the leader substituted alone
    emitter.instruction("movzx r8d, BYTE PTR [rsi + rcx + 3]");                 // load the third continuation
    emitter.instruction("mov r10d, r8d");                                       // copy the third continuation
    emitter.instruction("and r10d, 0xC0");                                      // isolate the continuation prefix
    emitter.instruction("cmp r10d, 0x80");                                      // is the third continuation well-formed?
    emitter.instruction("jne __rt_mb_cc_decode_raw1_x86");                      // malformed continuation leaves the leader substituted alone
    emitter.instruction("and eax, 0x07");                                       // keep the four-byte payload bits
    emitter.instruction("shl eax, 18");                                         // shift the leader payload into place
    emitter.instruction("and r9d, 0x3F");                                       // keep the first continuation payload
    emitter.instruction("shl r9d, 12");                                         // shift the first continuation into place
    emitter.instruction("and r11d, 0x3F");                                      // keep the second continuation payload
    emitter.instruction("shl r11d, 6");                                         // shift the second continuation into place
    emitter.instruction("and r8d, 0x3F");                                       // keep the third continuation payload
    emitter.instruction("or eax, r9d");                                         // assemble the high bits
    emitter.instruction("or eax, r11d");                                        // assemble the mid bits
    emitter.instruction("or eax, r8d");                                         // assemble the scalar
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // store the four-byte scalar
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // mark the unit as a scalar
    emitter.instruction("mov QWORD PTR [rbp - 96], 4");                         // consume all four bytes
    emitter.instruction("ret");                                                 // return the four-byte scalar

    emitter.label("__rt_mb_cc_decode_trunc_x86");
    emitter.instruction("mov QWORD PTR [rbp - 88], 1");                         // truncated prefixes are raw units
    emitter.instruction("mov QWORD PTR [rbp - 96], r8");                        // consume every remaining byte in the truncated group
    emitter.instruction("ret");                                                 // return the truncated raw unit
}

/// Emits case mapping, title-state updates, final-sigma, and output encoding.
fn emit_apply_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_cc_apply_x86");
    emitter.instruction("sub rsp, 8");                                          // keep nested calls 16-byte aligned from this frameless entry
    emitter.instruction("mov eax, DWORD PTR [rbp - 80]");                       // load the decoded scalar
    emitter.instruction("cmp eax, 0x03A3");                                     // is this Greek capital sigma?
    emitter.instruction("jne __rt_mb_cc_map_x86");                              // ordinary scalars use the mapping tables
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // load the case mode
    emitter.instruction("cmp rcx, 1");                                          // MB_CASE_LOWER uses final sigma
    emitter.instruction("je __rt_mb_cc_sigma_check_x86");                       // check the word-boundary rule
    emitter.instruction("cmp rcx, 2");                                          // MB_CASE_TITLE uses final sigma only inside a word
    emitter.instruction("jne __rt_mb_cc_map_x86");                              // other modes keep capital sigma
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // title_mode must already be inside a word
    emitter.instruction("je __rt_mb_cc_map_x86");                               // word-initial sigma is title-cased normally
    emitter.label("__rt_mb_cc_sigma_check_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // was the previous non-ignorable letter cased?
    emitter.instruction("je __rt_mb_cc_map_x86");                               // isolated sigma is not final
    emitter.instruction("call __rt_mb_cc_sigma_ahead_x86");                     // look ahead past Case_Ignorable
    emitter.instruction("test rax, rax");                                       // is this the last cased letter in the word?
    emitter.instruction("jz __rt_mb_cc_map_x86");                               // a later cased letter keeps capital/lowercase mapping
    emitter.instruction("mov DWORD PTR [rbp - 244], 0x03C2");                   // emit Greek final sigma
    emitter.instruction("mov DWORD PTR [rbp - 232], 1");                        // one output code point
    emitter.instruction("jmp __rt_mb_cc_emit_mapped_x86");                      // encode the final sigma

    emitter.label("__rt_mb_cc_map_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // load the case mode
    emitter.instruction("mov eax, DWORD PTR [rbp - 80]");                       // reload the scalar
    emitter.instruction("cmp rcx, 4");                                          // simple modes are 4..=7
    emitter.instruction("jae __rt_mb_cc_map_simple_x86");                       // simple mappings stay 1:1
    emitter.instruction("cmp rcx, 0");                                          // MB_CASE_UPPER
    emitter.instruction("je __rt_mb_cc_map_full_upper_x86");                    // full uppercase may expand
    emitter.instruction("cmp rcx, 2");                                          // MB_CASE_TITLE
    emitter.instruction("je __rt_mb_cc_map_full_title_x86");                    // title uses titlecase or lowercase
    emitter.instruction("cmp rcx, 3");                                          // MB_CASE_FOLD
    emitter.instruction("je __rt_mb_cc_map_full_fold_x86");                     // full case fold may expand ß and ligatures
    emitter.label("__rt_mb_cc_map_full_lower_x86");
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_full_lower");
    emitter.instruction("mov edi, eax");                                        // look up a 1:N lowercase expansion
    emitter.instruction("call __rt_mb_cc_full_lookup_x86");                     // rax = mapped length or zero
    emitter.instruction("test rax, rax");                                       // did a 1:N lowercase mapping hit?
    emitter.instruction("jnz __rt_mb_cc_emit_mapped_x86");                      // emit the expansion
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_simple_lower");
    emitter.instruction("mov edi, DWORD PTR [rbp - 80]");                       // fall back to the 1:1 lowercase table
    emitter.instruction("call __rt_mb_cc_simple_lookup_x86");                   // rax = mapped or original code
    emitter.instruction("mov DWORD PTR [rbp - 244], eax");                      // store the single mapped code
    emitter.instruction("mov DWORD PTR [rbp - 232], 1");                        // one output code point
    emitter.instruction("jmp __rt_mb_cc_emit_mapped_x86");                      // encode the mapped scalar

    emitter.label("__rt_mb_cc_map_full_fold_x86");
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_full_fold");
    emitter.instruction("mov edi, eax");                                        // look up a 1:N case-fold expansion
    emitter.instruction("call __rt_mb_cc_full_lookup_x86");                     // rax = mapped length or zero
    emitter.instruction("test rax, rax");                                       // did a 1:N fold mapping hit?
    emitter.instruction("jnz __rt_mb_cc_emit_mapped_x86");                      // emit the expansion
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_simple_lower");
    emitter.instruction("mov edi, DWORD PTR [rbp - 80]");                       // fold falls back to 1:1 lowercase
    emitter.instruction("call __rt_mb_cc_simple_lookup_x86");                   // rax = mapped or original code
    emitter.instruction("mov DWORD PTR [rbp - 244], eax");                      // store the single mapped code
    emitter.instruction("mov DWORD PTR [rbp - 232], 1");                        // one output code point
    emitter.instruction("jmp __rt_mb_cc_emit_mapped_x86");                      // encode the mapped scalar

    emitter.label("__rt_mb_cc_map_full_upper_x86");
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_full_upper");
    emitter.instruction("mov edi, eax");                                        // look up a 1:N uppercase expansion
    emitter.instruction("call __rt_mb_cc_full_lookup_x86");                     // rax = mapped length or zero
    emitter.instruction("test rax, rax");                                       // did a 1:N uppercase mapping hit?
    emitter.instruction("jnz __rt_mb_cc_emit_mapped_x86");                      // emit the expansion
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_simple_upper");
    emitter.instruction("mov edi, DWORD PTR [rbp - 80]");                       // fall back to the 1:1 uppercase table
    emitter.instruction("call __rt_mb_cc_simple_lookup_x86");                   // rax = mapped or original code
    emitter.instruction("mov DWORD PTR [rbp - 244], eax");                      // store the single mapped code
    emitter.instruction("mov DWORD PTR [rbp - 232], 1");                        // one output code point
    emitter.instruction("jmp __rt_mb_cc_emit_mapped_x86");                      // encode the mapped scalar

    emitter.label("__rt_mb_cc_map_full_title_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // title_mode selects lowercase vs titlecase
    emitter.instruction("jne __rt_mb_cc_map_full_lower_x86");                   // later letters in a word are lowercased
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_full_title");
    emitter.instruction("mov edi, eax");                                        // look up a 1:N titlecase expansion
    emitter.instruction("call __rt_mb_cc_full_lookup_x86");                     // rax = mapped length or zero
    emitter.instruction("test rax, rax");                                       // did a 1:N titlecase mapping hit?
    emitter.instruction("jnz __rt_mb_cc_emit_mapped_x86");                      // emit the expansion
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_simple_title");
    emitter.instruction("mov edi, DWORD PTR [rbp - 80]");                       // fall back to the 1:1 titlecase table
    emitter.instruction("call __rt_mb_cc_simple_lookup_x86");                   // rax = mapped or original code
    emitter.instruction("mov DWORD PTR [rbp - 244], eax");                      // store the single mapped code
    emitter.instruction("mov DWORD PTR [rbp - 232], 1");                        // one output code point
    emitter.instruction("jmp __rt_mb_cc_emit_mapped_x86");                      // encode the mapped scalar

    emitter.label("__rt_mb_cc_map_simple_x86");
    emitter.instruction("cmp rcx, 4");                                          // MB_CASE_UPPER_SIMPLE
    emitter.instruction("je __rt_mb_cc_map_simple_upper_x86");                  // 1:1 uppercase
    emitter.instruction("cmp rcx, 6");                                          // MB_CASE_TITLE_SIMPLE
    emitter.instruction("je __rt_mb_cc_map_simple_title_x86");                  // 1:1 titlecase
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_simple_lower");
    emitter.instruction("jmp __rt_mb_cc_map_simple_run_x86");                   // LOWER_SIMPLE and FOLD_SIMPLE share lowercase

    emitter.label("__rt_mb_cc_map_simple_upper_x86");
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_simple_upper");
    emitter.instruction("jmp __rt_mb_cc_map_simple_run_x86");                   // look up the 1:1 uppercase mapping

    emitter.label("__rt_mb_cc_map_simple_title_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // title_mode selects lowercase vs titlecase
    emitter.instruction("je __rt_mb_cc_map_simple_title_head_x86");             // word-initial letters use titlecase
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_simple_lower");
    emitter.instruction("jmp __rt_mb_cc_map_simple_run_x86");                   // later letters in a word are lowercased
    emitter.label("__rt_mb_cc_map_simple_title_head_x86");
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_simple_title");

    emitter.label("__rt_mb_cc_map_simple_run_x86");
    emitter.instruction("mov edi, DWORD PTR [rbp - 80]");                       // look up the 1:1 mapping
    emitter.instruction("call __rt_mb_cc_simple_lookup_x86");                   // rax = mapped or original code
    emitter.instruction("mov DWORD PTR [rbp - 244], eax");                      // store the single mapped code
    emitter.instruction("mov DWORD PTR [rbp - 232], 1");                        // one output code point

    emitter.label("__rt_mb_cc_emit_mapped_x86");
    emitter.instruction("mov DWORD PTR [rbp - 228], 0");                        // output-code index starts at zero
    emitter.label("__rt_mb_cc_emit_mapped_loop_x86");
    emitter.instruction("mov r11d, DWORD PTR [rbp - 228]");                     // reload the output-code index
    emitter.instruction("cmp r11d, DWORD PTR [rbp - 232]");                     // emitted every mapped code point?
    emitter.instruction("jae __rt_mb_cc_update_title_x86");                     // update title state after encoding
    emitter.instruction("mov edi, DWORD PTR [rbp - 244 + r11 * 4]");            // load the next mapped code point
    emitter.instruction("call __rt_mb_cc_encode_x86");                          // write UTF-8 or a Latin-1 byte
    emitter.instruction("inc DWORD PTR [rbp - 228]");                           // advance the output-code index
    emitter.instruction("jmp __rt_mb_cc_emit_mapped_loop_x86");                 // encode the remaining mapped codes

    emitter.label("__rt_mb_cc_update_title_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the source offset
    emitter.instruction("add rax, QWORD PTR [rbp - 96]");                       // consume the decoded unit
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // persist the advanced source offset
    emitter.instruction("mov edi, DWORD PTR [rbp - 80]");                       // test Case_Ignorable on the original scalar
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_ignorable");
    emitter.instruction("call __rt_mb_cc_in_range_x86");                        // rax = 1 when the original scalar is ignorable
    emitter.instruction("test rax, rax");                                       // Case_Ignorable leaves title_mode unchanged
    emitter.instruction("jnz __rt_mb_cc_apply_ret_x86");                        // skip title/prev updates for ignorable marks
    emitter.instruction("mov edi, DWORD PTR [rbp - 80]");                       // test Cased on the original scalar
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_cased");
    emitter.instruction("call __rt_mb_cc_in_range_x86");                        // rax = 1 when the original scalar is cased
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // prev_cased tracks the last non-ignorable letter
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the case mode
    emitter.instruction("cmp rcx, 2");                                          // MB_CASE_TITLE updates title_mode
    emitter.instruction("je __rt_mb_cc_store_title_x86");                       // store is_cased as the new title_mode
    emitter.instruction("cmp rcx, 6");                                          // MB_CASE_TITLE_SIMPLE updates title_mode
    emitter.instruction("jne __rt_mb_cc_apply_ret_x86");                        // other modes ignore title_mode
    emitter.label("__rt_mb_cc_store_title_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // title_mode becomes is_cased of this scalar
    emitter.label("__rt_mb_cc_apply_ret_x86");
    emitter.instruction("add rsp, 8");                                          // release the nested-call alignment pad
    emitter.instruction("ret");                                                 // return to the convert loop

    emitter.label("__rt_mb_cc_sigma_ahead_x86");
    emitter.instruction("sub rsp, 8");                                          // keep nested calls 16-byte aligned from this frameless entry
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // start the lookahead after the current unit
    emitter.instruction("add rax, QWORD PTR [rbp - 96]");                       // skip the current sigma
    emitter.instruction("mov QWORD PTR [rbp - 152], rax");                      // peek offset
    emitter.label("__rt_mb_cc_sigma_ahead_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 152]");                      // reload the peek offset
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // reached the end of the source?
    emitter.instruction("jae __rt_mb_cc_sigma_yes_x86");                        // EOF means this sigma is final
    emitter.instruction("push QWORD PTR [rbp - 48]");                           // preserve the real source offset
    emitter.instruction("push QWORD PTR [rbp - 80]");                           // preserve the current scalar
    emitter.instruction("push QWORD PTR [rbp - 88]");                           // preserve the current unit kind
    emitter.instruction("push QWORD PTR [rbp - 96]");                           // preserve the current consume count
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // decode at the peek offset
    emitter.instruction("call __rt_mb_cc_decode_x86");                          // decode the peeked unit
    emitter.instruction("mov r8, QWORD PTR [rbp - 88]");                        // load the peeked unit kind
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");                        // load the peeked scalar
    emitter.instruction("mov r10, QWORD PTR [rbp - 96]");                       // load the peeked consume count
    emitter.instruction("pop QWORD PTR [rbp - 96]");                            // restore the current consume count
    emitter.instruction("pop QWORD PTR [rbp - 88]");                            // restore the current unit kind
    emitter.instruction("pop QWORD PTR [rbp - 80]");                            // restore the current scalar
    emitter.instruction("pop QWORD PTR [rbp - 48]");                            // restore the real source offset
    emitter.instruction("cmp r8, 1");                                           // a raw unit ends the word
    emitter.instruction("je __rt_mb_cc_sigma_yes_x86");                         // malformed bytes count as a word boundary
    emitter.instruction("mov QWORD PTR [rbp - 160], r10");                      // spill the peeked consume count without changing rsp
    emitter.instruction("mov edi, r9d");                                        // test Case_Ignorable on the peeked scalar
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_ignorable");
    emitter.instruction("call __rt_mb_cc_in_range_x86");                        // rax = 1 when the peek is ignorable
    emitter.instruction("mov r10, QWORD PTR [rbp - 160]");                      // restore the peeked consume count
    emitter.instruction("test rax, rax");                                       // skip Case_Ignorable marks
    emitter.instruction("jz __rt_mb_cc_sigma_cased_x86");                       // a non-ignorable peek decides the word
    emitter.instruction("add QWORD PTR [rbp - 152], r10");                      // advance the peek offset past the mark
    emitter.instruction("jmp __rt_mb_cc_sigma_ahead_loop_x86");                 // keep scanning
    emitter.label("__rt_mb_cc_sigma_cased_x86");
    emitter.instruction("mov edi, r9d");                                        // test Cased on the peeked scalar
    abi::emit_symbol_address(emitter, "rsi", "_mb_cc_cased");
    emitter.instruction("call __rt_mb_cc_in_range_x86");                        // rax = 1 when the peek is cased
    emitter.instruction("xor rax, 1");                                          // final sigma when the next letter is not cased
    emitter.instruction("add rsp, 8");                                          // release the nested-call alignment pad
    emitter.instruction("ret");                                                 // return the lookahead answer
    emitter.label("__rt_mb_cc_sigma_yes_x86");
    emitter.instruction("mov rax, 1");                                          // EOF / raw units make this sigma final
    emitter.instruction("add rsp, 8");                                          // release the nested-call alignment pad
    emitter.instruction("ret");                                                 // return true
}

/// Emits binary-search helpers for range, simple, and full mapping tables.
fn emit_table_helpers_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_cc_in_range_x86");
    emitter.instruction("mov ecx, DWORD PTR [rsi]");                            // load the range count
    emitter.instruction("add rsi, 4");                                          // point at the first (lo, hi) pair
    emitter.instruction("xor edx, edx");                                        // binary-search lo index
    emitter.label("__rt_mb_cc_in_range_loop_x86");
    emitter.instruction("cmp edx, ecx");                                        // empty remaining window?
    emitter.instruction("jae __rt_mb_cc_in_range_miss_x86");                    // the code is outside every range
    emitter.instruction("mov r8d, edx");                                        // mid = lo
    emitter.instruction("add r8d, ecx");                                        // mid = lo + hi
    emitter.instruction("shr r8d, 1");                                          // mid = (lo + hi) / 2
    emitter.instruction("mov r9d, r8d");                                        // copy mid
    emitter.instruction("shl r9d, 3");                                          // byte offset = mid * 8
    emitter.instruction("mov r10d, DWORD PTR [rsi + r9]");                      // load range lo
    emitter.instruction("cmp edi, r10d");                                       // code < lo?
    emitter.instruction("jb __rt_mb_cc_in_range_left_x86");                     // search the lower half
    emitter.instruction("mov r10d, DWORD PTR [rsi + r9 + 4]");                  // load range hi
    emitter.instruction("cmp edi, r10d");                                       // code > hi?
    emitter.instruction("ja __rt_mb_cc_in_range_right_x86");                    // search the upper half
    emitter.instruction("mov rax, 1");                                          // the code sits inside this range
    emitter.instruction("ret");                                                 // return found
    emitter.label("__rt_mb_cc_in_range_left_x86");
    emitter.instruction("mov ecx, r8d");                                        // hi = mid
    emitter.instruction("jmp __rt_mb_cc_in_range_loop_x86");                    // continue the search
    emitter.label("__rt_mb_cc_in_range_right_x86");
    emitter.instruction("mov edx, r8d");                                        // lo = mid
    emitter.instruction("inc edx");                                             // lo = mid + 1
    emitter.instruction("jmp __rt_mb_cc_in_range_loop_x86");                    // continue the search
    emitter.label("__rt_mb_cc_in_range_miss_x86");
    emitter.instruction("xor eax, eax");                                        // not found
    emitter.instruction("ret");                                                 // return zero

    emitter.label("__rt_mb_cc_simple_lookup_x86");
    emitter.instruction("mov ecx, DWORD PTR [rsi]");                            // load the pair count
    emitter.instruction("add rsi, 4");                                          // point at the first (from, to) pair
    emitter.instruction("xor edx, edx");                                        // binary-search lo index
    emitter.instruction("mov r11d, edi");                                       // remember the original code as the miss result
    emitter.label("__rt_mb_cc_simple_loop_x86");
    emitter.instruction("cmp edx, ecx");                                        // empty remaining window?
    emitter.instruction("jae __rt_mb_cc_simple_miss_x86");                      // return the original code
    emitter.instruction("mov r8d, edx");                                        // mid = lo
    emitter.instruction("add r8d, ecx");                                        // mid = lo + hi
    emitter.instruction("shr r8d, 1");                                          // mid = (lo + hi) / 2
    emitter.instruction("mov r9d, r8d");                                        // copy mid
    emitter.instruction("shl r9d, 3");                                          // byte offset = mid * 8
    emitter.instruction("mov r10d, DWORD PTR [rsi + r9]");                      // load the from code
    emitter.instruction("cmp edi, r10d");                                       // compare against the search key
    emitter.instruction("je __rt_mb_cc_simple_hit_x86");                        // return the mapped to-code
    emitter.instruction("jb __rt_mb_cc_simple_left_x86");                       // search the lower half
    emitter.instruction("mov edx, r8d");                                        // lo = mid
    emitter.instruction("inc edx");                                             // lo = mid + 1
    emitter.instruction("jmp __rt_mb_cc_simple_loop_x86");                      // continue the search
    emitter.label("__rt_mb_cc_simple_left_x86");
    emitter.instruction("mov ecx, r8d");                                        // hi = mid
    emitter.instruction("jmp __rt_mb_cc_simple_loop_x86");                      // continue the search
    emitter.label("__rt_mb_cc_simple_hit_x86");
    emitter.instruction("mov eax, DWORD PTR [rsi + r9 + 4]");                   // load the mapped to-code
    emitter.instruction("ret");                                                 // return the 1:1 mapping
    emitter.label("__rt_mb_cc_simple_miss_x86");
    emitter.instruction("mov eax, r11d");                                       // identity mapping
    emitter.instruction("ret");                                                 // return the original code

    emitter.label("__rt_mb_cc_full_lookup_x86");
    emitter.instruction("mov ecx, DWORD PTR [rsi]");                            // load the expansion count
    emitter.instruction("add rsi, 4");                                          // point at the first 5-word entry
    emitter.instruction("xor edx, edx");                                        // binary-search lo index
    emitter.label("__rt_mb_cc_full_loop_x86");
    emitter.instruction("cmp edx, ecx");                                        // empty remaining window?
    emitter.instruction("jae __rt_mb_cc_full_miss_x86");                        // no 1:N mapping
    emitter.instruction("mov r8d, edx");                                        // mid = lo
    emitter.instruction("add r8d, ecx");                                        // mid = lo + hi
    emitter.instruction("shr r8d, 1");                                          // mid = (lo + hi) / 2
    emitter.instruction("mov r9d, r8d");                                        // copy mid
    emitter.instruction("imul r9d, 20");                                        // byte offset = mid * 20
    emitter.instruction("mov r10d, DWORD PTR [rsi + r9]");                      // load the from code
    emitter.instruction("cmp edi, r10d");                                       // compare against the search key
    emitter.instruction("je __rt_mb_cc_full_hit_x86");                          // copy the expansion into the frame
    emitter.instruction("jb __rt_mb_cc_full_left_x86");                         // search the lower half
    emitter.instruction("mov edx, r8d");                                        // lo = mid
    emitter.instruction("inc edx");                                             // lo = mid + 1
    emitter.instruction("jmp __rt_mb_cc_full_loop_x86");                        // continue the search
    emitter.label("__rt_mb_cc_full_left_x86");
    emitter.instruction("mov ecx, r8d");                                        // hi = mid
    emitter.instruction("jmp __rt_mb_cc_full_loop_x86");                        // continue the search
    emitter.label("__rt_mb_cc_full_hit_x86");
    emitter.instruction("mov eax, DWORD PTR [rsi + r9 + 4]");                   // load the expansion length
    emitter.instruction("mov DWORD PTR [rbp - 232], eax");                      // store map_len
    emitter.instruction("mov r10d, DWORD PTR [rsi + r9 + 8]");                  // load mapped code 0
    emitter.instruction("mov DWORD PTR [rbp - 244], r10d");                     // store map0
    emitter.instruction("mov r10d, DWORD PTR [rsi + r9 + 12]");                 // load mapped code 1
    emitter.instruction("mov DWORD PTR [rbp - 240], r10d");                     // store map1
    emitter.instruction("mov r10d, DWORD PTR [rsi + r9 + 16]");                 // load mapped code 2
    emitter.instruction("mov DWORD PTR [rbp - 236], r10d");                     // store map2
    emitter.instruction("ret");                                                 // return the expansion length
    emitter.label("__rt_mb_cc_full_miss_x86");
    emitter.instruction("xor eax, eax");                                        // no expansion
    emitter.instruction("ret");                                                 // return zero
}

/// Emits one output code point as UTF-8, or as a single byte in 8bit mode when it fits.
fn emit_encode_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_cc_encode_x86");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // load the destination cursor
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // UTF-8 destination?
    emitter.instruction("je __rt_mb_cc_encode_utf8_x86");                       // encode a Unicode scalar
    emitter.instruction("cmp edi, 0xFF");                                       // does the mapped code still fit in one byte?
    emitter.instruction("ja __rt_mb_cc_encode_utf8_x86");                       // expanded Latin-1 results fall back to UTF-8
    emitter.instruction("mov BYTE PTR [rdx], dil");                             // store the Latin-1 byte
    emitter.instruction("inc rdx");                                             // advance the destination cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // persist the destination cursor
    emitter.instruction("ret");                                                 // return after the Latin-1 store

    emitter.label("__rt_mb_cc_encode_utf8_x86");
    emitter.instruction("cmp edi, 0x80");                                       // ASCII scalars are one byte
    emitter.instruction("jae __rt_mb_cc_encode_2_x86");                         // encode a multi-byte scalar
    emitter.instruction("mov BYTE PTR [rdx], dil");                             // store the ASCII byte
    emitter.instruction("inc rdx");                                             // advance the destination cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // persist the destination cursor
    emitter.instruction("ret");                                                 // return after the ASCII store

    emitter.label("__rt_mb_cc_encode_2_x86");
    emitter.instruction("cmp edi, 0x800");                                      // two-byte scalars are below U+0800
    emitter.instruction("jae __rt_mb_cc_encode_3_x86");                         // encode a three- or four-byte scalar
    emitter.instruction("mov eax, edi");                                        // copy the scalar
    emitter.instruction("shr eax, 6");                                          // high bits
    emitter.instruction("or eax, 0xC0");                                        // two-byte leader
    emitter.instruction("mov BYTE PTR [rdx], al");                              // store the leader
    emitter.instruction("mov eax, edi");                                        // reload the scalar
    emitter.instruction("and eax, 0x3F");                                       // low six bits
    emitter.instruction("or eax, 0x80");                                        // continuation
    emitter.instruction("mov BYTE PTR [rdx + 1], al");                          // store the continuation
    emitter.instruction("add rdx, 2");                                          // advance by two bytes
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // persist the destination cursor
    emitter.instruction("ret");                                                 // return after the two-byte store

    emitter.label("__rt_mb_cc_encode_3_x86");
    emitter.instruction("cmp edi, 0x10000");                                    // three-byte scalars are below U+10000
    emitter.instruction("jae __rt_mb_cc_encode_4_x86");                         // encode a four-byte scalar
    emitter.instruction("mov eax, edi");                                        // copy the scalar
    emitter.instruction("shr eax, 12");                                         // high bits
    emitter.instruction("or eax, 0xE0");                                        // three-byte leader
    emitter.instruction("mov BYTE PTR [rdx], al");                              // store the leader
    emitter.instruction("mov eax, edi");                                        // reload the scalar
    emitter.instruction("shr eax, 6");                                          // mid bits
    emitter.instruction("and eax, 0x3F");                                       // six bits
    emitter.instruction("or eax, 0x80");                                        // continuation
    emitter.instruction("mov BYTE PTR [rdx + 1], al");                          // store the mid continuation
    emitter.instruction("mov eax, edi");                                        // reload the scalar
    emitter.instruction("and eax, 0x3F");                                       // low six bits
    emitter.instruction("or eax, 0x80");                                        // continuation
    emitter.instruction("mov BYTE PTR [rdx + 2], al");                          // store the last continuation
    emitter.instruction("add rdx, 3");                                          // advance by three bytes
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // persist the destination cursor
    emitter.instruction("ret");                                                 // return after the three-byte store

    emitter.label("__rt_mb_cc_encode_4_x86");
    emitter.instruction("mov eax, edi");                                        // copy the scalar
    emitter.instruction("shr eax, 18");                                         // high bits
    emitter.instruction("or eax, 0xF0");                                        // four-byte leader
    emitter.instruction("mov BYTE PTR [rdx], al");                              // store the leader
    emitter.instruction("mov eax, edi");                                        // reload the scalar
    emitter.instruction("shr eax, 12");                                         // next bits
    emitter.instruction("and eax, 0x3F");                                       // six bits
    emitter.instruction("or eax, 0x80");                                        // continuation
    emitter.instruction("mov BYTE PTR [rdx + 1], al");                          // store the first continuation
    emitter.instruction("mov eax, edi");                                        // reload the scalar
    emitter.instruction("shr eax, 6");                                          // next bits
    emitter.instruction("and eax, 0x3F");                                       // six bits
    emitter.instruction("or eax, 0x80");                                        // continuation
    emitter.instruction("mov BYTE PTR [rdx + 2], al");                          // store the second continuation
    emitter.instruction("mov eax, edi");                                        // reload the scalar
    emitter.instruction("and eax, 0x3F");                                       // low six bits
    emitter.instruction("or eax, 0x80");                                        // continuation
    emitter.instruction("mov BYTE PTR [rdx + 3], al");                          // store the last continuation
    emitter.instruction("add rdx, 4");                                          // advance by four bytes
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // persist the destination cursor
    emitter.instruction("ret");                                                 // return after the four-byte store
}
