//! Purpose:
//! AArch64 implementation of `__rt_mb_convert_case` for macOS and Linux.
//!
//! Called from:
//! - `super::emit_mb_convert_case()`.
//!
//! Key details:
//! - Arguments: `x1`/`x2` string, `x3` mode, `x4`/`x5` optional encoding.
//! - Result is reserved through `__rt_concat_reserve` and returned in `x1`/`x2`.
//! - `map_len` at `[x29, #-232]` and the emit index at `[x29, #-228]` are packed
//!   32-bit slots so a 64-bit store cannot overlap the neighboring word.

use super::{MAX_ENCODING_NAME_LEN, RESERVE_MULTIPLIER};
use crate::codegen_support::{
    abi,
    emit::Emitter,
    platform::Platform,
    runtime::{arrays::value_error, data::MB_CONVERT_CASE_BAD_ENCODING_MSG, data::MB_CONVERT_CASE_BAD_MODE_MSG},
};

/// Emits the AArch64 implementation for macOS and Linux.
pub(super) fn emit_mb_convert_case_aarch64(emitter: &mut Emitter) {
    let errno_function = match emitter.platform {
        Platform::MacOS => "__error",
        Platform::Linux => "__errno_location",
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    };

    emitter.blank();
    emitter.comment("--- runtime: mb_convert_case (PHP 8.5 Unicode case conversion) ---");
    emitter.label_global("__rt_mb_convert_case");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("sub sp, sp, #320");                                    // reserve convert state, iconv pointers, and the encoding-name buffer
    emitter.instruction("str x1, [x29, #-8]");                                  // save the source string pointer
    emitter.instruction("str x2, [x29, #-16]");                                 // save the source string length
    emitter.instruction("str x3, [x29, #-24]");                                 // save the MB_CASE_* mode integer
    emitter.instruction("str x4, [x29, #-136]");                                // save the optional encoding pointer
    emitter.instruction("str x5, [x29, #-144]");                                // save the optional encoding length
    emitter.instruction("str wzr, [x29, #-220]");                               // iconv reverse-encode is off unless a non-UTF-8 name was used
    emitter.instruction(&format!(
        "cmp x3, #{}",
        crate::types::string_constants::MB_CASE_MODE_MAX
    ));                                                                         // is the mode one of PHP's MB_CASE_* constants?
    emitter.instruction("b.hi __rt_mb_cc_bad_mode");                            // reject modes outside 0..=7 with ValueError
    emitter.instruction("cbz x4, __rt_mb_cc_utf8");                             // omitted/null encoding uses UTF-8
    emitter.instruction(&format!("cmp x5, #{}", MAX_ENCODING_NAME_LEN));        // does the encoding name fit the stack C-string buffer?
    emitter.instruction("b.hi __rt_mb_cc_bad_encoding");                        // reject names longer than every PHP-supported alias
    emitter.instruction("sub x6, x29, #320");                                   // destination is the 64-byte encoding-name buffer
    emitter.instruction("mov x7, xzr");                                         // copied-byte index starts at zero
    emitter.label("__rt_mb_cc_enc_copy");
    emitter.instruction("cmp x7, x5");                                          // copied the whole explicit encoding name?
    emitter.instruction("b.hs __rt_mb_cc_enc_copied");                          // terminate the C string once every byte is copied
    emitter.instruction("ldrb w8, [x4, x7]");                                   // load one encoding-name byte from the PHP string
    emitter.instruction("strb w8, [x6, x7]");                                   // append the byte to the stack C string
    emitter.instruction("add x7, x7, #1");                                      // advance the encoding-name byte index
    emitter.instruction("b __rt_mb_cc_enc_copy");                               // continue copying the remaining encoding-name bytes
    emitter.label("__rt_mb_cc_enc_copied");
    emitter.instruction("strb wzr, [x6, x5]");                                  // NUL-terminate the explicit encoding name
    emit_enc_cmp_aarch64(emitter, "_mb_strlen_utf8_name", "__rt_mb_cc_utf8");
    emit_enc_cmp_aarch64(emitter, "_mb_strlen_utf8_alias", "__rt_mb_cc_utf8");
    emit_enc_cmp_aarch64(emitter, "_mb_strlen_8bit_name", "__rt_mb_cc_latin1");
    emit_enc_cmp_aarch64(emitter, "_mb_strlen_binary_name", "__rt_mb_cc_latin1");
    emit_enc_cmp_aarch64(emitter, "_mb_strlen_7bit_name", "__rt_mb_cc_latin1");
    emitter.instruction("b __rt_mb_cc_iconv");                                  // remaining names decode through libc iconv

    emitter.label("__rt_mb_cc_utf8");
    emitter.instruction("str xzr, [x29, #-72]");                                // UTF-8 mode decodes validated Unicode scalars
    emitter.instruction("b __rt_mb_cc_convert");                                // convert the saved source string
    emitter.label("__rt_mb_cc_latin1");
    emitter.instruction("mov x8, #1");                                          // 8bit/binary/7bit treat every byte as U+00xx
    emitter.instruction("str x8, [x29, #-72]");                                 // persist the Latin-1 decoder flag

    emitter.label("__rt_mb_cc_convert");
    emitter.instruction("ldr x0, [x29, #-16]");                                 // load the source length before computing the reservation
    emitter.instruction(&format!("lsl x8, x0, #2"));                            // reserve four output bytes per input byte
    let _ = RESERVE_MULTIPLIER;
    emitter.instruction("mov x0, x8");                                          // pass the reservation size
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the converted string
    emitter.instruction("str x0, [x29, #-32]");                                 // remember the destination start pointer
    emitter.instruction("str x0, [x29, #-40]");                                 // destination cursor starts at the reserved buffer
    emitter.instruction("str xzr, [x29, #-48]");                                // source byte offset starts at zero
    emitter.instruction("str xzr, [x29, #-56]");                                // title_mode starts false
    emitter.instruction("str xzr, [x29, #-64]");                                // previous non-ignorable cased flag starts false

    emitter.label("__rt_mb_cc_loop");
    emitter.instruction("ldr x8, [x29, #-48]");                                 // reload the current source offset
    emitter.instruction("ldr x9, [x29, #-16]");                                 // reload the source length
    emitter.instruction("cmp x8, x9");                                          // have we consumed every source byte?
    emitter.instruction("b.hs __rt_mb_cc_done");                                // publish once the source is exhausted
    emitter.instruction("bl __rt_mb_cc_decode");                                // decode the next UTF-8 or 8-bit unit
    emitter.instruction("ldr x8, [x29, #-88]");                                 // load the unit kind
    emitter.instruction("cmp x8, #1");                                          // was the unit a malformed/raw byte group?
    emitter.instruction("b.eq __rt_mb_cc_raw");                                 // copy malformed bytes through unchanged
    emitter.instruction("bl __rt_mb_cc_apply");                                 // map the scalar and emit UTF-8 or Latin-1 bytes
    emitter.instruction("b __rt_mb_cc_loop");                                   // continue with the next source unit

    emitter.label("__rt_mb_cc_raw");
    emitter.instruction("ldr x1, [x29, #-8]");                                  // load the source pointer
    emitter.instruction("ldr x8, [x29, #-48]");                                 // load the source offset
    emitter.instruction("add x1, x1, x8");                                      // point at the raw byte group
    emitter.instruction("ldr x2, [x29, #-40]");                                 // load the destination cursor
    emitter.instruction("ldr x3, [x29, #-96]");                                 // load the raw group length
    emitter.label("__rt_mb_cc_raw_copy");
    emitter.instruction("cbz x3, __rt_mb_cc_raw_done");                         // finished copying the raw group
    emitter.instruction("ldrb w4, [x1], #1");                                   // load one raw byte
    emitter.instruction("strb w4, [x2], #1");                                   // store one raw byte
    emitter.instruction("sub x3, x3, #1");                                      // decrement the remaining raw count
    emitter.instruction("b __rt_mb_cc_raw_copy");                               // copy the next raw byte
    emitter.label("__rt_mb_cc_raw_done");
    emitter.instruction("str x2, [x29, #-40]");                                 // store the advanced destination cursor
    emitter.instruction("ldr x8, [x29, #-48]");                                 // reload the source offset
    emitter.instruction("ldr x9, [x29, #-96]");                                 // load the raw group length
    emitter.instruction("add x8, x8, x9");                                      // consume the raw group
    emitter.instruction("str x8, [x29, #-48]");                                 // persist the advanced source offset
    emitter.instruction("str xzr, [x29, #-56]");                                // raw units reset title_mode
    emitter.instruction("str xzr, [x29, #-64]");                                // raw units are non-cased and non-ignorable
    emitter.instruction("b __rt_mb_cc_loop");                                   // continue with the next source unit

    emitter.label("__rt_mb_cc_done");
    emitter.instruction("ldr w8, [x29, #-220]");                                // does the result still need encoding back from UTF-8?
    emitter.instruction("cbnz w8, __rt_mb_cc_iconv_from_utf8");                 // reverse-encode iconv results into the original encoding
    emitter.label("__rt_mb_cc_publish");
    emitter.instruction("ldr x1, [x29, #-32]");                                 // result pointer is the reserved destination start
    emitter.instruction("ldr x2, [x29, #-40]");                                 // load the destination cursor
    emitter.instruction("sub x2, x2, x1");                                      // x2 = dest_cur - dest_start
    emitter.instruction("bl __rt_concat_publish");                              // publish scratch-backed results
    emitter.instruction("mov sp, x29");                                         // release the helper frame
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame and return address
    emitter.instruction("ret");                                                 // return the converted string

    emit_iconv_aarch64(emitter, errno_function);
    emit_decode_aarch64(emitter);
    emit_apply_aarch64(emitter);
    emit_table_helpers_aarch64(emitter);
    emit_encode_aarch64(emitter);

    emitter.label("__rt_mb_cc_bad_mode");
    emitter.instruction("mov sp, x29");                                         // release the helper frame before throwing
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame before throwing
    value_error::emit_throw_value_error_aarch64(
        emitter,
        "_mb_convert_case_bad_mode_msg",
        MB_CONVERT_CASE_BAD_MODE_MSG.len(),
    );
    emitter.label("__rt_mb_cc_bad_encoding");
    emitter.instruction("mov sp, x29");                                         // release the helper frame before throwing
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame before throwing
    value_error::emit_throw_value_error_aarch64(
        emitter,
        "_mb_convert_case_bad_encoding_msg",
        MB_CONVERT_CASE_BAD_ENCODING_MSG.len(),
    );
}

/// Compares the copied encoding name against one alias and branches on a match.
fn emit_enc_cmp_aarch64(emitter: &mut Emitter, symbol: &str, hit: &str) {
    emitter.instruction("sub x0, x29, #320");                                   // first strcasecmp argument is the copied encoding name
    abi::emit_symbol_address(emitter, "x1", symbol);
    emitter.bl_c("strcasecmp"); // compare the explicit encoding case-insensitively
    emitter.instruction(&format!("cbz x0, {hit}"));                             // equal → encoding match
}

/// Emits iconv-backed conversion for encodings other than UTF-8 and 8bit aliases.
fn emit_iconv_aarch64(emitter: &mut Emitter, errno_function: &str) {
    let _ = errno_function;
    emitter.label("__rt_mb_cc_iconv");
    abi::emit_symbol_address(emitter, "x0", "_mb_strlen_utf8_name");
    emitter.instruction("sub x1, x29, #320");                                   // iconv source encoding is the copied explicit name
    emitter.bl_c("iconv_open"); // open a decoder from the requested encoding into UTF-8
    emitter.instruction("cmn x0, #1");                                          // did iconv_open return the failure sentinel?
    emitter.instruction("b.eq __rt_mb_cc_bad_encoding");                        // unknown encoding names raise PHP's ValueError
    emitter.instruction("str x0, [x29, #-168]");                                // preserve the to-UTF-8 iconv descriptor
    emitter.instruction("ldr x0, [x29, #-16]");                                 // load the source length
    emitter.instruction("lsl x0, x0, #2");                                      // reserve four UTF-8 bytes per input byte
    emitter.instruction("add x0, x0, #16");                                     // keep a small slack buffer for the decoder
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate a temporary UTF-8 decode buffer
    emitter.instruction("str x0, [x29, #-184]");                                // save the UTF-8 decode buffer pointer
    emitter.instruction("str x0, [x29, #-192]");                                // mutable outbuf pointer starts at the UTF-8 buffer
    emitter.instruction("ldr x8, [x29, #-16]");                                 // reload the original source length
    emitter.instruction("lsl x8, x8, #2");                                      // output capacity matches the allocated UTF-8 buffer
    emitter.instruction("add x8, x8, #16");                                     // include the slack bytes
    emitter.instruction("str x8, [x29, #-176]");                                // outbytesleft starts at the allocated capacity
    emitter.label("__rt_mb_cc_iconv_to_utf8");
    emitter.instruction("ldr x8, [x29, #-16]");                                 // any input bytes remaining?
    emitter.instruction("cbz x8, __rt_mb_cc_iconv_to_utf8_done");               // close the decoder after the input is consumed
    emitter.instruction("ldr x0, [x29, #-168]");                                // reload the decoder descriptor
    emitter.instruction("sub x1, x29, #8");                                     // inbuf pointer slot
    emitter.instruction("sub x2, x29, #16");                                    // inbytesleft slot
    emitter.instruction("sub x3, x29, #192");                                   // outbuf pointer slot
    emitter.instruction("sub x4, x29, #176");                                   // outbytesleft slot
    emitter.bl_c("iconv"); // decode the next UTF-8 chunk
    emitter.instruction("cmn x0, #1");                                          // did iconv report an error?
    emitter.instruction("b.ne __rt_mb_cc_iconv_to_utf8");                       // successful progress continues until input is exhausted
    emitter.instruction("ldr x8, [x29, #-8]");                                  // load the current input pointer
    emitter.instruction("ldrb w9, [x8]");                                       // copy the malformed byte into the UTF-8 buffer
    emitter.instruction("ldr x10, [x29, #-192]");                               // load the current UTF-8 output pointer
    emitter.instruction("strb w9, [x10]");                                      // store the copied malformed byte
    emitter.instruction("add x10, x10, #1");                                    // advance the UTF-8 output pointer
    emitter.instruction("str x10, [x29, #-192]");                               // persist the UTF-8 output pointer
    emitter.instruction("add x8, x8, #1");                                      // skip the malformed input byte
    emitter.instruction("str x8, [x29, #-8]");                                  // persist the advanced input pointer
    emitter.instruction("ldr x8, [x29, #-16]");                                 // reload remaining input
    emitter.instruction("sub x8, x8, #1");                                      // consume the malformed input byte
    emitter.instruction("str x8, [x29, #-16]");                                 // persist the reduced input count
    emitter.instruction("b __rt_mb_cc_iconv_to_utf8");                          // continue decoding after the malformed byte
    emitter.label("__rt_mb_cc_iconv_to_utf8_done");
    emitter.instruction("ldr x0, [x29, #-168]");                                // reload the decoder descriptor
    emitter.bl_c("iconv_close"); // release the to-UTF-8 descriptor
    emitter.instruction("ldr x8, [x29, #-192]");                                // load the UTF-8 write cursor
    emitter.instruction("ldr x9, [x29, #-184]");                                // load the UTF-8 buffer start
    emitter.instruction("sub x8, x8, x9");                                      // compute the decoded UTF-8 length
    emitter.instruction("str x9, [x29, #-8]");                                  // source pointer is the temporary UTF-8 buffer
    emitter.instruction("str x8, [x29, #-16]");                                 // source length is the decoded UTF-8 length
    emitter.instruction("str xzr, [x29, #-72]");                                // decoded iconv bytes are UTF-8
    emitter.instruction("mov w8, #1");                                          // convert back to the original encoding after UTF-8 mapping
    emitter.instruction("str w8, [x29, #-220]");                                // request a reverse encode after conversion
    emitter.instruction("b __rt_mb_cc_convert");                                // reuse the UTF-8 converter

    emitter.label("__rt_mb_cc_iconv_from_utf8");
    emitter.instruction("sub x0, x29, #320");                                   // original encoding name
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_utf8_name");
    emitter.bl_c("iconv_open"); // open an encoder from UTF-8 back to the original encoding
    emitter.instruction("cmn x0, #1");                                          // did the reverse conversion reject the encoding?
    emitter.instruction("b.eq __rt_mb_cc_bad_encoding");                        // treat a failed reverse open as an unknown encoding
    emitter.instruction("str x0, [x29, #-168]");                                // preserve the from-UTF-8 iconv descriptor
    emitter.instruction("ldr x8, [x29, #-32]");                                 // converted UTF-8 pointer
    emitter.instruction("str x8, [x29, #-8]");                                  // iconv input is the converted UTF-8 string
    emitter.instruction("ldr x9, [x29, #-40]");                                 // converted UTF-8 cursor
    emitter.instruction("sub x9, x9, x8");                                      // converted UTF-8 length
    emitter.instruction("str x9, [x29, #-16]");                                 // iconv input length
    emitter.instruction("lsl x0, x9, #2");                                      // reserve four bytes per converted UTF-8 byte
    emitter.instruction("add x0, x0, #16");                                     // keep slack for the encoder
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the reverse-encoded result
    emitter.instruction("str x0, [x29, #-184]");                                // save the reverse-encoded buffer
    emitter.instruction("str x0, [x29, #-192]");                                // mutable outbuf starts at that buffer
    emitter.instruction("ldr x8, [x29, #-16]");                                 // reload the converted UTF-8 length
    emitter.instruction("lsl x8, x8, #2");                                      // output capacity
    emitter.instruction("add x8, x8, #16");                                     // include slack
    emitter.instruction("str x8, [x29, #-176]");                                // outbytesleft
    emitter.label("__rt_mb_cc_iconv_from_loop");
    emitter.instruction("ldr x8, [x29, #-16]");                                 // any converted UTF-8 bytes remaining?
    emitter.instruction("cbz x8, __rt_mb_cc_iconv_from_done");                   // finish after the converted UTF-8 is consumed
    emitter.instruction("ldr x0, [x29, #-168]");                                // reload the encoder descriptor
    emitter.instruction("sub x1, x29, #8");                                     // inbuf pointer slot
    emitter.instruction("sub x2, x29, #16");                                    // inbytesleft slot
    emitter.instruction("sub x3, x29, #192");                                   // outbuf pointer slot
    emitter.instruction("sub x4, x29, #176");                                   // outbytesleft slot
    emitter.bl_c("iconv"); // encode the next chunk
    emitter.instruction("cmn x0, #1");                                          // did iconv report an error?
    emitter.instruction("b.eq __rt_mb_cc_iconv_from_done");                      // stop on encoder errors and publish what was written
    emitter.instruction("b __rt_mb_cc_iconv_from_loop");                        // successful progress continues
    emitter.label("__rt_mb_cc_iconv_from_done");
    emitter.instruction("ldr x0, [x29, #-168]");                                // reload the encoder descriptor
    emitter.bl_c("iconv_close"); // release the from-UTF-8 descriptor
    emitter.instruction("ldr x8, [x29, #-184]");                                // reverse-encoded pointer
    emitter.instruction("ldr x9, [x29, #-192]");                                // reverse-encoded cursor
    emitter.instruction("str x8, [x29, #-32]");                                 // publish this buffer as the result pointer
    emitter.instruction("str x9, [x29, #-40]");                                 // dest_cur is the reverse-encoded cursor
    emitter.instruction("str wzr, [x29, #-220]");                               // prevent a second reverse-encode pass
    emitter.instruction("b __rt_mb_cc_publish");                                // publish the reverse-encoded string
}

/// Emits UTF-8 / 8-bit decode of the unit at the current source offset.
fn emit_decode_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_cc_decode");
    emitter.instruction("ldr x1, [x29, #-8]");                                  // load the source pointer
    emitter.instruction("ldr x2, [x29, #-48]");                                 // load the current source offset
    emitter.instruction("ldr x3, [x29, #-16]");                                 // load the source length
    emitter.instruction("ldr x8, [x29, #-72]");                                 // load the Latin-1 flag
    emitter.instruction("cbz x8, __rt_mb_cc_decode_utf8");                      // UTF-8 validates multi-byte sequences
    emitter.instruction("ldrb w0, [x1, x2]");                                   // 8bit reads the next byte as U+00xx
    emitter.instruction("str x0, [x29, #-80]");                                 // store the Latin-1 code point
    emitter.instruction("str xzr, [x29, #-88]");                                // 8bit units are always scalars
    emitter.instruction("mov x8, #1");                                          // each 8bit unit consumes one byte
    emitter.instruction("str x8, [x29, #-96]");                                 // persist the consume count
    emitter.instruction("ret");                                                 // return the decoded 8-bit unit

    emitter.label("__rt_mb_cc_decode_utf8");
    emitter.instruction("ldrb w0, [x1, x2]");                                   // load the next possible UTF-8 leading byte
    emitter.instruction("cmp w0, #0x80");                                       // ASCII bytes are complete one-byte characters
    emitter.instruction("b.lo __rt_mb_cc_decode_ascii");                        // consume one ASCII scalar
    emitter.instruction("cmp w0, #0xC2");                                       // C0/C1 and continuation bytes are malformed leaders
    emitter.instruction("b.lo __rt_mb_cc_decode_raw1");                         // substitute one malformed byte
    emitter.instruction("cmp w0, #0xE0");                                       // C2-DF begin two-byte characters
    emitter.instruction("b.lo __rt_mb_cc_decode_two");                          // validate a two-byte character
    emitter.instruction("cmp w0, #0xF0");                                       // E0-EF begin three-byte characters
    emitter.instruction("b.lo __rt_mb_cc_decode_three");                        // validate a three-byte character
    emitter.instruction("cmp w0, #0xF5");                                       // F0-F4 begin four-byte characters
    emitter.instruction("b.lo __rt_mb_cc_decode_four");                         // validate a four-byte character
    emitter.label("__rt_mb_cc_decode_raw1");
    emitter.instruction("mov x8, #1");                                          // mark the unit as a raw malformed group
    emitter.instruction("str x8, [x29, #-88]");                                 // persist the raw kind
    emitter.instruction("str x8, [x29, #-96]");                                 // consume the malformed leader alone
    emitter.instruction("ret");                                                 // return the raw unit
    emitter.label("__rt_mb_cc_decode_ascii");
    emitter.instruction("str x0, [x29, #-80]");                                 // store the ASCII code point
    emitter.instruction("str xzr, [x29, #-88]");                                // ASCII units are scalars
    emitter.instruction("mov x8, #1");                                          // ASCII consumes one byte
    emitter.instruction("str x8, [x29, #-96]");                                 // persist the consume count
    emitter.instruction("ret");                                                 // return the ASCII scalar

    emitter.label("__rt_mb_cc_decode_two");
    emitter.instruction("sub x8, x3, x2");                                      // remaining bytes including the leader
    emitter.instruction("cmp x8, #2");                                          // is a complete two-byte sequence present?
    emitter.instruction("b.lo __rt_mb_cc_decode_trunc");                        // group a truncated valid prefix as one raw unit
    emitter.instruction("add x9, x1, x2");                                      // point at the leader
    emitter.instruction("ldrb w10, [x9, #1]");                                  // load the continuation byte
    emitter.instruction("and w11, w10, #0xC0");                                 // isolate the continuation prefix
    emitter.instruction("cmp w11, #0x80");                                      // is it a well-formed continuation?
    emitter.instruction("b.ne __rt_mb_cc_decode_raw1");                         // malformed continuation leaves the leader substituted alone
    emitter.instruction("and w0, w0, #0x1F");                                   // keep the two-byte payload bits
    emitter.instruction("lsl w0, w0, #6");                                      // shift the leader payload into place
    emitter.instruction("and w10, w10, #0x3F");                                 // keep the continuation payload
    emitter.instruction("orr w0, w0, w10");                                     // assemble the scalar
    emitter.instruction("str x0, [x29, #-80]");                                 // store the two-byte scalar
    emitter.instruction("str xzr, [x29, #-88]");                                // mark the unit as a scalar
    emitter.instruction("mov x8, #2");                                          // consume both bytes
    emitter.instruction("str x8, [x29, #-96]");                                 // persist the consume count
    emitter.instruction("ret");                                                 // return the two-byte scalar

    emitter.label("__rt_mb_cc_decode_three");
    emitter.instruction("sub x8, x3, x2");                                      // remaining bytes including the leader
    emitter.instruction("cmp x8, #3");                                          // is a complete three-byte sequence present?
    emitter.instruction("b.lo __rt_mb_cc_decode_trunc");                        // group a truncated valid prefix as one raw unit
    emitter.instruction("add x9, x1, x2");                                      // point at the leader
    emitter.instruction("ldrb w10, [x9, #1]");                                  // load the first continuation
    emitter.instruction("ldrb w11, [x9, #2]");                                  // load the second continuation
    emitter.instruction("and w12, w10, #0xC0");                                 // isolate the first continuation prefix
    emitter.instruction("cmp w12, #0x80");                                      // is the first continuation well-formed?
    emitter.instruction("b.ne __rt_mb_cc_decode_raw1");                         // malformed continuation leaves the leader substituted alone
    emitter.instruction("and w12, w11, #0xC0");                                 // isolate the second continuation prefix
    emitter.instruction("cmp w12, #0x80");                                      // is the second continuation well-formed?
    emitter.instruction("b.ne __rt_mb_cc_decode_raw1");                         // malformed tail leaves the leader substituted alone
    emitter.instruction("cmp w0, #0xE0");                                       // E0 needs a lower-bound check
    emitter.instruction("b.ne __rt_mb_cc_decode_three_ed");                     // skip the E0 bound for other leaders
    emitter.instruction("cmp w10, #0xA0");                                      // reject overlong three-byte sequences
    emitter.instruction("b.lo __rt_mb_cc_decode_raw1");                         // overlong encodings are malformed
    emitter.label("__rt_mb_cc_decode_three_ed");
    emitter.instruction("cmp w0, #0xED");                                       // ED needs a surrogate bound
    emitter.instruction("b.ne __rt_mb_cc_decode_three_ok");                     // skip the surrogate bound for other leaders
    emitter.instruction("cmp w10, #0xA0");                                      // reject UTF-8 encodings of surrogate code points
    emitter.instruction("b.hs __rt_mb_cc_decode_raw1");                         // surrogate encodings are malformed
    emitter.label("__rt_mb_cc_decode_three_ok");
    emitter.instruction("and w0, w0, #0x0F");                                   // keep the three-byte payload bits
    emitter.instruction("lsl w0, w0, #12");                                     // shift the leader payload into place
    emitter.instruction("and w10, w10, #0x3F");                                 // keep the first continuation payload
    emitter.instruction("lsl w10, w10, #6");                                    // shift the first continuation into place
    emitter.instruction("and w11, w11, #0x3F");                                 // keep the second continuation payload
    emitter.instruction("orr w0, w0, w10");                                     // assemble the high bits
    emitter.instruction("orr w0, w0, w11");                                     // assemble the scalar
    emitter.instruction("str x0, [x29, #-80]");                                 // store the three-byte scalar
    emitter.instruction("str xzr, [x29, #-88]");                                // mark the unit as a scalar
    emitter.instruction("mov x8, #3");                                          // consume all three bytes
    emitter.instruction("str x8, [x29, #-96]");                                 // persist the consume count
    emitter.instruction("ret");                                                 // return the three-byte scalar

    emitter.label("__rt_mb_cc_decode_four");
    emitter.instruction("sub x8, x3, x2");                                      // remaining bytes including the leader
    emitter.instruction("cmp x8, #4");                                          // is a complete four-byte sequence present?
    emitter.instruction("b.lo __rt_mb_cc_decode_trunc");                        // group a truncated valid prefix as one raw unit
    emitter.instruction("add x9, x1, x2");                                      // point at the leader
    emitter.instruction("ldrb w10, [x9, #1]");                                  // load the first continuation
    emitter.instruction("and w12, w10, #0xC0");                                 // isolate the first continuation prefix
    emitter.instruction("cmp w12, #0x80");                                      // is the first continuation well-formed?
    emitter.instruction("b.ne __rt_mb_cc_decode_raw1");                         // malformed continuation leaves the leader substituted alone
    emitter.instruction("cmp w0, #0xF0");                                       // F0 needs a lower-bound check
    emitter.instruction("b.ne __rt_mb_cc_decode_four_f4");                      // skip the F0 bound for other leaders
    emitter.instruction("cmp w10, #0x90");                                      // reject overlong four-byte sequences
    emitter.instruction("b.lo __rt_mb_cc_decode_raw1");                         // overlong encodings are malformed
    emitter.label("__rt_mb_cc_decode_four_f4");
    emitter.instruction("cmp w0, #0xF4");                                       // F4 needs an upper bound
    emitter.instruction("b.ne __rt_mb_cc_decode_four_cont");                     // F0-F3 continue
    emitter.instruction("cmp w10, #0x90");                                      // reject out-of-range four-byte sequences
    emitter.instruction("b.hs __rt_mb_cc_decode_raw1");                         // code points above U+10FFFF are malformed
    emitter.label("__rt_mb_cc_decode_four_cont");
    emitter.instruction("ldrb w11, [x9, #2]");                                  // load the second continuation
    emitter.instruction("ldrb w13, [x9, #3]");                                  // load the third continuation
    emitter.instruction("and w12, w11, #0xC0");                                 // isolate the second continuation prefix
    emitter.instruction("cmp w12, #0x80");                                      // is the second continuation well-formed?
    emitter.instruction("b.ne __rt_mb_cc_decode_raw1");                         // malformed continuation leaves the leader substituted alone
    emitter.instruction("and w12, w13, #0xC0");                                 // isolate the third continuation prefix
    emitter.instruction("cmp w12, #0x80");                                      // is the third continuation well-formed?
    emitter.instruction("b.ne __rt_mb_cc_decode_raw1");                         // malformed continuation leaves the leader substituted alone
    emitter.instruction("and w0, w0, #0x07");                                   // keep the four-byte payload bits
    emitter.instruction("lsl w0, w0, #18");                                     // shift the leader payload into place
    emitter.instruction("and w10, w10, #0x3F");                                 // keep the first continuation payload
    emitter.instruction("lsl w10, w10, #12");                                   // shift the first continuation into place
    emitter.instruction("and w11, w11, #0x3F");                                 // keep the second continuation payload
    emitter.instruction("lsl w11, w11, #6");                                    // shift the second continuation into place
    emitter.instruction("and w13, w13, #0x3F");                                 // keep the third continuation payload
    emitter.instruction("orr w0, w0, w10");                                     // assemble the high bits
    emitter.instruction("orr w0, w0, w11");                                     // assemble the mid bits
    emitter.instruction("orr w0, w0, w13");                                     // assemble the scalar
    emitter.instruction("str x0, [x29, #-80]");                                 // store the four-byte scalar
    emitter.instruction("str xzr, [x29, #-88]");                                // mark the unit as a scalar
    emitter.instruction("mov x8, #4");                                          // consume all four bytes
    emitter.instruction("str x8, [x29, #-96]");                                 // persist the consume count
    emitter.instruction("ret");                                                 // return the four-byte scalar

    emitter.label("__rt_mb_cc_decode_trunc");
    emitter.instruction("mov x9, #1");                                          // truncated prefixes are raw units
    emitter.instruction("str x9, [x29, #-88]");                                 // persist the raw kind
    emitter.instruction("str x8, [x29, #-96]");                                 // consume every remaining byte in the truncated group
    emitter.instruction("ret");                                                 // return the truncated raw unit
}

/// Emits case mapping, title-state updates, final-sigma, and output encoding.
fn emit_apply_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_cc_apply");
    emitter.instruction("ldr w0, [x29, #-80]");                                 // load the decoded scalar
    emitter.instruction("mov w8, #0x03A3");                                     // Greek capital sigma
    emitter.instruction("cmp w0, w8");                                          // is this Greek capital sigma?
    emitter.instruction("b.ne __rt_mb_cc_map");                                 // ordinary scalars use the mapping tables
    emitter.instruction("ldr x1, [x29, #-24]");                                 // load the case mode
    emitter.instruction("cmp x1, #1");                                          // MB_CASE_LOWER uses final sigma
    emitter.instruction("b.eq __rt_mb_cc_sigma_check");                         // check the word-boundary rule
    emitter.instruction("cmp x1, #2");                                          // MB_CASE_TITLE uses final sigma only inside a word
    emitter.instruction("b.ne __rt_mb_cc_map");                                 // other modes keep capital sigma
    emitter.instruction("ldr x8, [x29, #-56]");                                 // title_mode must already be inside a word
    emitter.instruction("cbz x8, __rt_mb_cc_map");                              // word-initial sigma is title-cased normally
    emitter.label("__rt_mb_cc_sigma_check");
    emitter.instruction("ldr x8, [x29, #-64]");                                 // was the previous non-ignorable letter cased?
    emitter.instruction("cbz x8, __rt_mb_cc_map");                              // isolated sigma is not final
    emitter.instruction("bl __rt_mb_cc_sigma_ahead");                           // look ahead past Case_Ignorable
    emitter.instruction("cbz x0, __rt_mb_cc_map");                              // a later cased letter keeps capital/lowercase mapping
    emitter.instruction("mov w8, #0x03C2");                                     // emit Greek final sigma
    emitter.instruction("str w8, [x29, #-244]");                                // store the mapped code
    emitter.instruction("mov x8, #1");                                          // one output code point
    emitter.instruction("str w8, [x29, #-232]");                                // persist map_len
    emitter.instruction("b __rt_mb_cc_emit_mapped");                            // encode the final sigma

    emitter.label("__rt_mb_cc_map");
    emitter.instruction("ldr x1, [x29, #-24]");                                 // load the case mode
    emitter.instruction("ldr w0, [x29, #-80]");                                 // reload the scalar
    emitter.instruction("cmp x1, #4");                                          // simple modes are 4..=7
    emitter.instruction("b.hs __rt_mb_cc_map_simple");                          // simple mappings stay 1:1
    emitter.instruction("cbz x1, __rt_mb_cc_map_full_upper");                   // MB_CASE_UPPER
    emitter.instruction("cmp x1, #2");                                          // MB_CASE_TITLE
    emitter.instruction("b.eq __rt_mb_cc_map_full_title");                      // title uses titlecase or lowercase
    emitter.instruction("cmp x1, #3");                                          // MB_CASE_FOLD
    emitter.instruction("b.eq __rt_mb_cc_map_full_fold");                       // full case fold may expand ß and ligatures
    emitter.label("__rt_mb_cc_map_full_lower");
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_full_lower");
    emitter.instruction("bl __rt_mb_cc_full_lookup");                           // x0 = mapped length or zero
    emitter.instruction("cbnz x0, __rt_mb_cc_emit_mapped");                     // emit the expansion
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_simple_lower");
    emitter.instruction("ldr w0, [x29, #-80]");                                 // fall back to the 1:1 lowercase table
    emitter.instruction("bl __rt_mb_cc_simple_lookup");                         // x0 = mapped or original code
    emitter.instruction("str w0, [x29, #-244]");                                // store the single mapped code
    emitter.instruction("mov x8, #1");                                          // one output code point
    emitter.instruction("str w8, [x29, #-232]");                                // persist map_len
    emitter.instruction("b __rt_mb_cc_emit_mapped");                            // encode the mapped scalar

    emitter.label("__rt_mb_cc_map_full_fold");
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_full_fold");
    emitter.instruction("bl __rt_mb_cc_full_lookup");                           // x0 = mapped length or zero
    emitter.instruction("cbnz x0, __rt_mb_cc_emit_mapped");                     // emit the expansion
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_simple_lower");
    emitter.instruction("ldr w0, [x29, #-80]");                                 // fold falls back to 1:1 lowercase
    emitter.instruction("bl __rt_mb_cc_simple_lookup");                         // x0 = mapped or original code
    emitter.instruction("str w0, [x29, #-244]");                                // store the single mapped code
    emitter.instruction("mov x8, #1");                                          // one output code point
    emitter.instruction("str w8, [x29, #-232]");                                // persist map_len
    emitter.instruction("b __rt_mb_cc_emit_mapped");                            // encode the mapped scalar

    emitter.label("__rt_mb_cc_map_full_upper");
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_full_upper");
    emitter.instruction("bl __rt_mb_cc_full_lookup");                           // x0 = mapped length or zero
    emitter.instruction("cbnz x0, __rt_mb_cc_emit_mapped");                     // emit the expansion
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_simple_upper");
    emitter.instruction("ldr w0, [x29, #-80]");                                 // fall back to the 1:1 uppercase table
    emitter.instruction("bl __rt_mb_cc_simple_lookup");                         // x0 = mapped or original code
    emitter.instruction("str w0, [x29, #-244]");                                // store the single mapped code
    emitter.instruction("mov x8, #1");                                          // one output code point
    emitter.instruction("str w8, [x29, #-232]");                                // persist map_len
    emitter.instruction("b __rt_mb_cc_emit_mapped");                            // encode the mapped scalar

    emitter.label("__rt_mb_cc_map_full_title");
    emitter.instruction("ldr x8, [x29, #-56]");                                 // title_mode selects lowercase vs titlecase
    emitter.instruction("cbnz x8, __rt_mb_cc_map_full_lower");                  // later letters in a word are lowercased
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_full_title");
    emitter.instruction("ldr w0, [x29, #-80]");                                 // look up a 1:N titlecase expansion
    emitter.instruction("bl __rt_mb_cc_full_lookup");                           // x0 = mapped length or zero
    emitter.instruction("cbnz x0, __rt_mb_cc_emit_mapped");                     // emit the expansion
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_simple_title");
    emitter.instruction("ldr w0, [x29, #-80]");                                 // fall back to the 1:1 titlecase table
    emitter.instruction("bl __rt_mb_cc_simple_lookup");                         // x0 = mapped or original code
    emitter.instruction("str w0, [x29, #-244]");                                // store the single mapped code
    emitter.instruction("mov x8, #1");                                          // one output code point
    emitter.instruction("str w8, [x29, #-232]");                                // persist map_len
    emitter.instruction("b __rt_mb_cc_emit_mapped");                            // encode the mapped scalar

    emitter.label("__rt_mb_cc_map_simple");
    emitter.instruction("cmp x1, #4");                                          // MB_CASE_UPPER_SIMPLE
    emitter.instruction("b.eq __rt_mb_cc_map_simple_upper");                    // 1:1 uppercase
    emitter.instruction("cmp x1, #6");                                          // MB_CASE_TITLE_SIMPLE
    emitter.instruction("b.eq __rt_mb_cc_map_simple_title");                    // 1:1 titlecase
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_simple_lower");
    emitter.instruction("b __rt_mb_cc_map_simple_run");                         // LOWER_SIMPLE and FOLD_SIMPLE share lowercase
    emitter.label("__rt_mb_cc_map_simple_upper");
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_simple_upper");
    emitter.instruction("b __rt_mb_cc_map_simple_run");                         // look up the 1:1 uppercase mapping
    emitter.label("__rt_mb_cc_map_simple_title");
    emitter.instruction("ldr x8, [x29, #-56]");                                 // title_mode selects lowercase vs titlecase
    emitter.instruction("cbz x8, __rt_mb_cc_map_simple_title_head");            // word-initial letters use titlecase
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_simple_lower");
    emitter.instruction("b __rt_mb_cc_map_simple_run");                         // later letters in a word are lowercased
    emitter.label("__rt_mb_cc_map_simple_title_head");
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_simple_title");
    emitter.label("__rt_mb_cc_map_simple_run");
    emitter.instruction("ldr w0, [x29, #-80]");                                 // look up the 1:1 mapping
    emitter.instruction("bl __rt_mb_cc_simple_lookup");                         // x0 = mapped or original code
    emitter.instruction("str w0, [x29, #-244]");                                // store the single mapped code
    emitter.instruction("mov x8, #1");                                          // one output code point
    emitter.instruction("str w8, [x29, #-232]");                                // persist map_len

    emitter.label("__rt_mb_cc_emit_mapped");
    emitter.instruction("str wzr, [x29, #-228]");                               // output-code index starts at zero
    emitter.label("__rt_mb_cc_emit_mapped_loop");
    emitter.instruction("ldr w8, [x29, #-228]");                                // reload the output-code index
    emitter.instruction("ldr w9, [x29, #-232]");                                // reload map_len
    emitter.instruction("cmp w8, w9");                                          // emitted every mapped code point?
    emitter.instruction("b.hs __rt_mb_cc_update_title");                        // update title state after encoding
    emitter.instruction("sub x10, x29, #244");                                  // point at map0
    emitter.instruction("ldr w0, [x10, x8, lsl #2]");                           // load the next mapped code point
    emitter.instruction("bl __rt_mb_cc_encode");                                // write UTF-8 or a Latin-1 byte
    emitter.instruction("ldr w8, [x29, #-228]");                                // reload the output-code index
    emitter.instruction("add w8, w8, #1");                                      // advance the output-code index
    emitter.instruction("str w8, [x29, #-228]");                                // persist the output-code index
    emitter.instruction("b __rt_mb_cc_emit_mapped_loop");                       // encode the remaining mapped codes

    emitter.label("__rt_mb_cc_update_title");
    emitter.instruction("ldr x8, [x29, #-48]");                                 // reload the source offset
    emitter.instruction("ldr x9, [x29, #-96]");                                 // load the consume count
    emitter.instruction("add x8, x8, x9");                                      // consume the decoded unit
    emitter.instruction("str x8, [x29, #-48]");                                 // persist the advanced source offset
    emitter.instruction("ldr w0, [x29, #-80]");                                 // test Case_Ignorable on the original scalar
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_ignorable");
    emitter.instruction("bl __rt_mb_cc_in_range");                              // x0 = 1 when the original scalar is ignorable
    emitter.instruction("cbnz x0, __rt_mb_cc_apply_ret");                       // skip title/prev updates for ignorable marks
    emitter.instruction("ldr w0, [x29, #-80]");                                 // test Cased on the original scalar
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_cased");
    emitter.instruction("bl __rt_mb_cc_in_range");                              // x0 = 1 when the original scalar is cased
    emitter.instruction("str x0, [x29, #-64]");                                 // prev_cased tracks the last non-ignorable letter
    emitter.instruction("ldr x1, [x29, #-24]");                                 // reload the case mode
    emitter.instruction("cmp x1, #2");                                          // MB_CASE_TITLE updates title_mode
    emitter.instruction("b.eq __rt_mb_cc_store_title");                         // store is_cased as the new title_mode
    emitter.instruction("cmp x1, #6");                                          // MB_CASE_TITLE_SIMPLE updates title_mode
    emitter.instruction("b.ne __rt_mb_cc_apply_ret");                           // other modes ignore title_mode
    emitter.label("__rt_mb_cc_store_title");
    emitter.instruction("str x0, [x29, #-56]");                                 // title_mode becomes is_cased of this scalar
    emitter.label("__rt_mb_cc_apply_ret");
    emitter.instruction("ret");                                                 // return to the convert loop

    emitter.label("__rt_mb_cc_sigma_ahead");
    emitter.instruction("ldr x8, [x29, #-48]");                                 // start the lookahead after the current unit
    emitter.instruction("ldr x9, [x29, #-96]");                                 // load the current consume count
    emitter.instruction("add x8, x8, x9");                                      // skip the current sigma
    emitter.instruction("str x8, [x29, #-152]");                                // peek offset
    emitter.label("__rt_mb_cc_sigma_ahead_loop");
    emitter.instruction("ldr x8, [x29, #-152]");                                // reload the peek offset
    emitter.instruction("ldr x9, [x29, #-16]");                                 // reload the source length
    emitter.instruction("cmp x8, x9");                                          // reached the end of the source?
    emitter.instruction("b.hs __rt_mb_cc_sigma_yes");                           // EOF means this sigma is final
    emitter.instruction("ldr x10, [x29, #-48]");                                // preserve the real source offset
    emitter.instruction("ldr x11, [x29, #-80]");                                // preserve the current scalar
    emitter.instruction("ldr x12, [x29, #-88]");                                // preserve the current unit kind
    emitter.instruction("ldr x13, [x29, #-96]");                                // preserve the current consume count
    emitter.instruction("stp x10, x11, [sp, #-32]!");                           // park offset and scalar
    emitter.instruction("stp x12, x13, [sp, #16]");                             // park kind and consume
    emitter.instruction("str x8, [x29, #-48]");                                 // decode at the peek offset
    emitter.instruction("bl __rt_mb_cc_decode");                                // decode the peeked unit
    emitter.instruction("ldr x11, [x29, #-88]");                                // load the peeked unit kind
    emitter.instruction("ldr x12, [x29, #-80]");                                // load the peeked scalar
    emitter.instruction("ldr x13, [x29, #-96]");                                // load the peeked consume count
    emitter.instruction("ldp x10, x14, [sp]");                                  // restore offset and scalar
    emitter.instruction("ldp x15, x16, [sp, #16]");                             // restore kind and consume
    emitter.instruction("add sp, sp, #32");                                     // release the parked decode state
    emitter.instruction("str x10, [x29, #-48]");                                // persist the real source offset
    emitter.instruction("str x14, [x29, #-80]");                                // persist the current scalar
    emitter.instruction("str x15, [x29, #-88]");                                // persist the current unit kind
    emitter.instruction("str x16, [x29, #-96]");                                // persist the current consume count
    emitter.instruction("cmp x11, #1");                                         // a raw unit ends the word
    emitter.instruction("b.eq __rt_mb_cc_sigma_yes");                           // malformed bytes count as a word boundary
    emitter.instruction("stp x12, x13, [sp, #-16]!");                           // park the peeked scalar and consume count
    emitter.instruction("mov w0, w12");                                         // test Case_Ignorable on the peeked scalar
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_ignorable");
    emitter.instruction("bl __rt_mb_cc_in_range");                              // x0 = 1 when the peek is ignorable
    emitter.instruction("ldp x12, x13, [sp], #16");                             // restore the peeked scalar and consume count
    emitter.instruction("cbz x0, __rt_mb_cc_sigma_cased");                      // a non-ignorable peek decides the word
    emitter.instruction("ldr x8, [x29, #-152]");                                // reload the peek offset
    emitter.instruction("add x8, x8, x13");                                     // advance the peek offset past the mark
    emitter.instruction("str x8, [x29, #-152]");                                // persist the peek offset
    emitter.instruction("b __rt_mb_cc_sigma_ahead_loop");                       // keep scanning
    emitter.label("__rt_mb_cc_sigma_cased");
    emitter.instruction("mov w0, w12");                                         // test Cased on the peeked scalar
    abi::emit_symbol_address(emitter, "x1", "_mb_cc_cased");
    emitter.instruction("bl __rt_mb_cc_in_range");                              // x0 = 1 when the peek is cased
    emitter.instruction("eor x0, x0, #1");                                      // final sigma when the next letter is not cased
    emitter.instruction("ret");                                                 // return the lookahead answer
    emitter.label("__rt_mb_cc_sigma_yes");
    emitter.instruction("mov x0, #1");                                          // EOF / raw units make this sigma final
    emitter.instruction("ret");                                                 // return true
}

/// Emits binary-search helpers for range, simple, and full mapping tables.
fn emit_table_helpers_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_cc_in_range");
    emitter.instruction("ldr w2, [x1]");                                        // load the range count
    emitter.instruction("add x1, x1, #4");                                      // point at the first (lo, hi) pair
    emitter.instruction("mov w3, wzr");                                         // binary-search lo index
    emitter.label("__rt_mb_cc_in_range_loop");
    emitter.instruction("cmp w3, w2");                                          // empty remaining window?
    emitter.instruction("b.hs __rt_mb_cc_in_range_miss");                       // the code is outside every range
    emitter.instruction("add w4, w3, w2");                                      // mid = lo + hi
    emitter.instruction("lsr w4, w4, #1");                                      // mid = (lo + hi) / 2
    emitter.instruction("add x5, x1, x4, lsl #3");                              // pointer to range mid
    emitter.instruction("ldr w6, [x5]");                                        // load range lo
    emitter.instruction("cmp w0, w6");                                          // code < lo?
    emitter.instruction("b.lo __rt_mb_cc_in_range_left");                       // search the lower half
    emitter.instruction("ldr w6, [x5, #4]");                                    // load range hi
    emitter.instruction("cmp w0, w6");                                          // code > hi?
    emitter.instruction("b.hi __rt_mb_cc_in_range_right");                      // search the upper half
    emitter.instruction("mov x0, #1");                                          // the code sits inside this range
    emitter.instruction("ret");                                                 // return found
    emitter.label("__rt_mb_cc_in_range_left");
    emitter.instruction("mov w2, w4");                                          // hi = mid
    emitter.instruction("b __rt_mb_cc_in_range_loop");                          // continue the search
    emitter.label("__rt_mb_cc_in_range_right");
    emitter.instruction("add w3, w4, #1");                                      // lo = mid + 1
    emitter.instruction("b __rt_mb_cc_in_range_loop");                          // continue the search
    emitter.label("__rt_mb_cc_in_range_miss");
    emitter.instruction("mov x0, xzr");                                         // not found
    emitter.instruction("ret");                                                 // return zero

    emitter.label("__rt_mb_cc_simple_lookup");
    emitter.instruction("ldr w2, [x1]");                                        // load the pair count
    emitter.instruction("add x1, x1, #4");                                      // point at the first (from, to) pair
    emitter.instruction("mov w3, wzr");                                         // binary-search lo index
    emitter.instruction("mov w7, w0");                                          // remember the original code as the miss result
    emitter.label("__rt_mb_cc_simple_loop");
    emitter.instruction("cmp w3, w2");                                          // empty remaining window?
    emitter.instruction("b.hs __rt_mb_cc_simple_miss");                         // return the original code
    emitter.instruction("add w4, w3, w2");                                      // mid = lo + hi
    emitter.instruction("lsr w4, w4, #1");                                      // mid = (lo + hi) / 2
    emitter.instruction("add x5, x1, x4, lsl #3");                              // pointer to pair mid
    emitter.instruction("ldr w6, [x5]");                                        // load the from code
    emitter.instruction("cmp w0, w6");                                          // compare against the search key
    emitter.instruction("b.eq __rt_mb_cc_simple_hit");                          // return the mapped to-code
    emitter.instruction("b.lo __rt_mb_cc_simple_left");                         // search the lower half
    emitter.instruction("add w3, w4, #1");                                      // lo = mid + 1
    emitter.instruction("b __rt_mb_cc_simple_loop");                            // continue the search
    emitter.label("__rt_mb_cc_simple_left");
    emitter.instruction("mov w2, w4");                                          // hi = mid
    emitter.instruction("b __rt_mb_cc_simple_loop");                            // continue the search
    emitter.label("__rt_mb_cc_simple_hit");
    emitter.instruction("ldr w0, [x5, #4]");                                    // load the mapped to-code
    emitter.instruction("ret");                                                 // return the 1:1 mapping
    emitter.label("__rt_mb_cc_simple_miss");
    emitter.instruction("mov w0, w7");                                          // identity mapping
    emitter.instruction("ret");                                                 // return the original code

    emitter.label("__rt_mb_cc_full_lookup");
    emitter.instruction("ldr w2, [x1]");                                        // load the expansion count
    emitter.instruction("add x1, x1, #4");                                      // point at the first 5-word entry
    emitter.instruction("mov w3, wzr");                                         // binary-search lo index
    emitter.label("__rt_mb_cc_full_loop");
    emitter.instruction("cmp w3, w2");                                          // empty remaining window?
    emitter.instruction("b.hs __rt_mb_cc_full_miss");                           // no 1:N mapping
    emitter.instruction("add w4, w3, w2");                                      // mid = lo + hi
    emitter.instruction("lsr w4, w4, #1");                                      // mid = (lo + hi) / 2
    emitter.instruction("mov w5, #20");                                         // entry size
    emitter.instruction("mul w6, w4, w5");                                      // byte offset = mid * 20
    emitter.instruction("add x5, x1, x6");                                      // pointer to entry mid
    emitter.instruction("ldr w6, [x5]");                                        // load the from code
    emitter.instruction("cmp w0, w6");                                          // compare against the search key
    emitter.instruction("b.eq __rt_mb_cc_full_hit");                            // copy the expansion into the frame
    emitter.instruction("b.lo __rt_mb_cc_full_left");                           // search the lower half
    emitter.instruction("add w3, w4, #1");                                      // lo = mid + 1
    emitter.instruction("b __rt_mb_cc_full_loop");                              // continue the search
    emitter.label("__rt_mb_cc_full_left");
    emitter.instruction("mov w2, w4");                                          // hi = mid
    emitter.instruction("b __rt_mb_cc_full_loop");                              // continue the search
    emitter.label("__rt_mb_cc_full_hit");
    emitter.instruction("ldr w0, [x5, #4]");                                    // load the expansion length
    emitter.instruction("str w0, [x29, #-232]");                                // store map_len
    emitter.instruction("ldr w6, [x5, #8]");                                    // load mapped code 0
    emitter.instruction("str w6, [x29, #-244]");                                // store map0
    emitter.instruction("ldr w6, [x5, #12]");                                   // load mapped code 1
    emitter.instruction("str w6, [x29, #-240]");                                // store map1
    emitter.instruction("ldr w6, [x5, #16]");                                   // load mapped code 2
    emitter.instruction("str w6, [x29, #-236]");                                // store map2
    emitter.instruction("ret");                                                 // return the expansion length
    emitter.label("__rt_mb_cc_full_miss");
    emitter.instruction("mov x0, xzr");                                         // no expansion
    emitter.instruction("ret");                                                 // return zero
}

/// Emits one output code point as UTF-8, or as a single byte in 8bit mode when it fits.
fn emit_encode_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_cc_encode");
    emitter.instruction("ldr x2, [x29, #-40]");                                 // load the destination cursor
    emitter.instruction("ldr x8, [x29, #-72]");                                 // UTF-8 destination?
    emitter.instruction("cbz x8, __rt_mb_cc_encode_utf8");                      // encode a Unicode scalar
    emitter.instruction("cmp w0, #0xFF");                                       // does the mapped code still fit in one byte?
    emitter.instruction("b.hi __rt_mb_cc_encode_utf8");                         // expanded Latin-1 results fall back to UTF-8
    emitter.instruction("strb w0, [x2]");                                       // store the Latin-1 byte
    emitter.instruction("add x2, x2, #1");                                      // advance the destination cursor
    emitter.instruction("str x2, [x29, #-40]");                                 // persist the destination cursor
    emitter.instruction("ret");                                                 // return after the Latin-1 store

    emitter.label("__rt_mb_cc_encode_utf8");
    emitter.instruction("cmp w0, #0x80");                                       // ASCII scalars are one byte
    emitter.instruction("b.hs __rt_mb_cc_encode_2");                            // encode a multi-byte scalar
    emitter.instruction("strb w0, [x2]");                                       // store the ASCII byte
    emitter.instruction("add x2, x2, #1");                                      // advance the destination cursor
    emitter.instruction("str x2, [x29, #-40]");                                 // persist the destination cursor
    emitter.instruction("ret");                                                 // return after the ASCII store

    emitter.label("__rt_mb_cc_encode_2");
    emitter.instruction("cmp w0, #0x800");                                      // two-byte scalars are below U+0800
    emitter.instruction("b.hs __rt_mb_cc_encode_3");                            // encode a three- or four-byte scalar
    emitter.instruction("lsr w8, w0, #6");                                      // high bits
    emitter.instruction("orr w8, w8, #0xC0");                                   // two-byte leader
    emitter.instruction("strb w8, [x2]");                                       // store the leader
    emitter.instruction("and w8, w0, #0x3F");                                   // low six bits
    emitter.instruction("orr w8, w8, #0x80");                                   // continuation
    emitter.instruction("strb w8, [x2, #1]");                                   // store the continuation
    emitter.instruction("add x2, x2, #2");                                      // advance by two bytes
    emitter.instruction("str x2, [x29, #-40]");                                 // persist the destination cursor
    emitter.instruction("ret");                                                 // return after the two-byte store

    emitter.label("__rt_mb_cc_encode_3");
    emitter.instruction("mov w8, #0x10000");                                    // three-byte scalars are below U+10000
    emitter.instruction("cmp w0, w8");                                          // compare against the four-byte boundary
    emitter.instruction("b.hs __rt_mb_cc_encode_4");                            // encode a four-byte scalar
    emitter.instruction("lsr w8, w0, #12");                                     // high bits
    emitter.instruction("orr w8, w8, #0xE0");                                   // three-byte leader
    emitter.instruction("strb w8, [x2]");                                       // store the leader
    emitter.instruction("lsr w8, w0, #6");                                      // mid bits
    emitter.instruction("and w8, w8, #0x3F");                                   // six bits
    emitter.instruction("orr w8, w8, #0x80");                                   // continuation
    emitter.instruction("strb w8, [x2, #1]");                                   // store the mid continuation
    emitter.instruction("and w8, w0, #0x3F");                                   // low six bits
    emitter.instruction("orr w8, w8, #0x80");                                   // continuation
    emitter.instruction("strb w8, [x2, #2]");                                   // store the last continuation
    emitter.instruction("add x2, x2, #3");                                      // advance by three bytes
    emitter.instruction("str x2, [x29, #-40]");                                 // persist the destination cursor
    emitter.instruction("ret");                                                 // return after the three-byte store

    emitter.label("__rt_mb_cc_encode_4");
    emitter.instruction("lsr w8, w0, #18");                                     // high bits
    emitter.instruction("orr w8, w8, #0xF0");                                   // four-byte leader
    emitter.instruction("strb w8, [x2]");                                       // store the leader
    emitter.instruction("lsr w8, w0, #12");                                     // next bits
    emitter.instruction("and w8, w8, #0x3F");                                   // six bits
    emitter.instruction("orr w8, w8, #0x80");                                   // continuation
    emitter.instruction("strb w8, [x2, #1]");                                   // store the first continuation
    emitter.instruction("lsr w8, w0, #6");                                      // next bits
    emitter.instruction("and w8, w8, #0x3F");                                   // six bits
    emitter.instruction("orr w8, w8, #0x80");                                   // continuation
    emitter.instruction("strb w8, [x2, #2]");                                   // store the second continuation
    emitter.instruction("and w8, w0, #0x3F");                                   // low six bits
    emitter.instruction("orr w8, w8, #0x80");                                   // continuation
    emitter.instruction("strb w8, [x2, #3]");                                   // store the last continuation
    emitter.instruction("add x2, x2, #4");                                      // advance by four bytes
    emitter.instruction("str x2, [x29, #-40]");                                 // persist the destination cursor
    emitter.instruction("ret");                                                 // return after the four-byte store
}
