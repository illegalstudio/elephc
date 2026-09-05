//! Purpose:
//! Emits the AArch64 `__rt_mb_strtoupper` helper for macOS, iOS, and Linux.
//!
//! Called from:
//! - `super::emit_mb_strtoupper()`.
//!
//! Key details:
//! - Encoding dispatch matches `mb_strlen`: omitted/null and UTF-8/UTF8 use the UTF-8
//!   walker, `8bit`/`binary`/`7bit` are ASCII-only, and other names go through iconv.
//! - UTF-8 uses Unicode full case mapping and copies malformed bytes through unchanged.

use crate::codegen_support::{
    abi,
    emit::Emitter,
    platform::Platform,
    runtime::{arrays::value_error, data::MB_STRTOUPPER_UNKNOWN_ENCODING_MSG},
};

/// Maximum explicit encoding-name length copied into the runtime's stack buffer.
const MAX_ENCODING_NAME_LEN: usize = 63;

/// Emits `__rt_mb_strtoupper(str_ptr, str_len, encoding_ptr, encoding_len) -> (ptr, len)`.
pub(super) fn emit_mb_strtoupper_aarch64(emitter: &mut Emitter) {
    let errno_function = match emitter.platform {
        Platform::MacOS => "__error",
        Platform::Linux => "__errno_location",
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    };

    emitter.blank();
    emitter.comment("--- runtime: mb_strtoupper (encoding-aware Unicode uppercase) ---");
    emitter.label_global("__rt_mb_strtoupper");
    emitter.instruction("sub sp, sp, #256");                                    // reserve dest state, encoding-name, case-out, and iconv slots
    emitter.instruction("stp x29, x30, [sp, #240]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #240");                                   // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the source string pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the source string length
    emitter.instruction("str xzr, [sp, #16]");                                  // source byte index starts at zero
    emitter.instruction("cbz x3, __rt_mb_strtoupper_utf8");                     // omitted/null encoding uses the default UTF-8 walker
    emitter.instruction(&format!("cmp x4, #{}", MAX_ENCODING_NAME_LEN));        // does the explicit encoding name fit the stack C-string buffer?
    emitter.instruction("b.hi __rt_mb_strtoupper_unknown_encoding");            // reject names longer than every PHP-supported encoding alias

    // -- copy the length-delimited PHP encoding name into a stack C string --
    emitter.instruction("add x9, sp, #80");                                     // destination is the 64-byte encoding-name buffer
    emitter.instruction("mov x10, #0");                                         // copied-byte index starts at zero
    emitter.label("__rt_mb_strtoupper_encoding_copy");
    emitter.instruction("cmp x10, x4");                                         // copied the whole explicit encoding name?
    emitter.instruction("b.hs __rt_mb_strtoupper_encoding_copied");             // terminate the C string once every byte is copied
    emitter.instruction("ldrb w11, [x3, x10]");                                 // load one encoding-name byte from the PHP string
    emitter.instruction("strb w11, [x9, x10]");                                 // append the byte to the stack C string
    emitter.instruction("add x10, x10, #1");                                    // advance the encoding-name byte index
    emitter.instruction("b __rt_mb_strtoupper_encoding_copy");                  // continue copying the remaining encoding-name bytes
    emitter.label("__rt_mb_strtoupper_encoding_copied");
    emitter.instruction("strb wzr, [x9, x4]");                                  // NUL-terminate the explicit encoding name

    emitter.instruction("add x0, sp, #80");                                     // first strcasecmp argument is the copied encoding name
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_utf8_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with UTF-8 case-insensitively
    emitter.instruction("cbz x0, __rt_mb_strtoupper_utf8");                     // UTF-8 uses the validated Unicode walker
    emitter.instruction("add x0, sp, #80");                                     // reload the copied encoding name after strcasecmp
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_utf8_alias");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with PHP's UTF8 alias
    emitter.instruction("cbz x0, __rt_mb_strtoupper_utf8");                     // the UTF8 alias uses the same walker
    emitter.instruction("add x0, sp, #80");                                     // reload the copied encoding name for the byte encodings
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_8bit_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with 8bit
    emitter.instruction("cbz x0, __rt_mb_strtoupper_bytes");                    // 8bit is ASCII-only uppercase
    emitter.instruction("add x0, sp, #80");                                     // reload the copied encoding name for the binary alias
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_binary_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with binary
    emitter.instruction("cbz x0, __rt_mb_strtoupper_bytes");                    // binary is PHP's alias for 8bit
    emitter.instruction("add x0, sp, #80");                                     // reload the copied encoding name for the 7bit encoding
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_7bit_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with 7bit
    emitter.instruction("cbz x0, __rt_mb_strtoupper_bytes");                    // 7bit preserves PHP's one-character-per-byte ASCII fold
    emitter.instruction("b __rt_mb_strtoupper_iconv");                          // remaining names decode through iconv

    emit_utf8_aarch64(emitter);
    emit_bytes_aarch64(emitter);
    emit_iconv_aarch64(emitter, errno_function);

    emitter.label("__rt_mb_strtoupper_empty");
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer write offset
    abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("add x1, x9, x10");                                     // empty results still need a valid pointer
    emitter.instruction("mov x2, #0");                                          // empty uppercase result has length zero
    emitter.instruction("ldp x29, x30, [sp, #240]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #256");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return the empty string

    emitter.label("__rt_mb_strtoupper_finish");
    emitter.instruction("ldr x1, [sp, #24]");                                   // result pointer is the reserved destination
    emitter.instruction("ldr x2, [sp, #40]");                                   // result length is the number of bytes written
    emitter.instruction("bl __rt_concat_publish");                              // advance concat scratch only for scratch-backed results
    emitter.instruction("ldp x29, x30, [sp, #240]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #256");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return the uppercase string

    emitter.label("__rt_mb_strtoupper_unknown_encoding");
    emitter.instruction("ldp x29, x30, [sp, #240]");                            // restore the caller frame before throwing ValueError
    emitter.instruction("add sp, sp, #256");                                    // release the helper frame before unwinding
    value_error::emit_throw_value_error_aarch64(
        emitter,
        "_mb_strtoupper_unknown_encoding_msg",
        MB_STRTOUPPER_UNKNOWN_ENCODING_MSG.len(),
    );

    emit_helpers_aarch64(emitter);
}

/// Emits the UTF-8 Unicode uppercase walker.
fn emit_utf8_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtoupper_utf8");
    emitter.instruction("ldr x2, [sp, #8]");                                    // load the source length for the empty-string fast path
    emitter.instruction("cbz x2, __rt_mb_strtoupper_empty");                    // empty input produces an empty result
    emitter.instruction("lsl x0, x2, #3");                                      // reserve eight times the source length for full-mapping growth
    emitter.instruction("str x0, [sp, #32]");                                   // remember the reserved destination capacity
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage
    emitter.instruction("str x0, [sp, #24]");                                   // save the destination pointer
    emitter.instruction("str xzr, [sp, #40]");                                  // no uppercase bytes have been written yet

    emitter.label("__rt_mb_strtoupper_utf8_loop");
    emitter.instruction("ldr x4, [sp, #16]");                                   // load the current source byte index
    emitter.instruction("ldr x2, [sp, #8]");                                    // load the source length
    emitter.instruction("cmp x4, x2");                                          // processed every source byte?
    emitter.instruction("b.hs __rt_mb_strtoupper_finish");                      // publish once the source is exhausted
    emitter.instruction("bl __rt_mb_strtoupper_ensure");                        // keep twelve free destination bytes for the next scalar
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source pointer
    emitter.instruction("ldr x4, [sp, #16]");                                   // reload the source byte index
    emitter.instruction("ldrb w5, [x1, x4]");                                   // load the next possible UTF-8 leading byte
    emitter.instruction("cmp w5, #0x80");                                       // ASCII bytes are complete one-byte characters
    emitter.instruction("b.lo __rt_mb_strtoupper_utf8_ascii");                  // uppercase one ASCII byte
    emitter.instruction("cmp w5, #0xC2");                                       // C0/C1 and continuation bytes are malformed leaders
    emitter.instruction("b.lo __rt_mb_strtoupper_utf8_invalid");                 // copy one malformed byte through
    emitter.instruction("cmp w5, #0xE0");                                       // C2-DF introduce two-byte sequences
    emitter.instruction("b.lo __rt_mb_strtoupper_utf8_two");                     // validate a two-byte character
    emitter.instruction("cmp w5, #0xF0");                                       // E0-EF introduce three-byte sequences
    emitter.instruction("b.lo __rt_mb_strtoupper_utf8_three");                   // validate a three-byte character
    emitter.instruction("cmp w5, #0xF5");                                       // F0-F4 introduce Unicode-range four-byte sequences
    emitter.instruction("b.lo __rt_mb_strtoupper_utf8_four");                    // validate a four-byte character
    emitter.instruction("b __rt_mb_strtoupper_utf8_invalid");                    // F5-FF cannot begin valid UTF-8

    emitter.label("__rt_mb_strtoupper_utf8_ascii");
    emitter.instruction("mov w0, w5");                                          // ASCII code point is the loaded byte
    emitter.instruction("mov x16, #1");                                         // one source byte is consumed
    emitter.instruction("b __rt_mb_strtoupper_utf8_map");                       // apply Unicode uppercase to the ASCII scalar

    emitter.label("__rt_mb_strtoupper_utf8_two");
    emitter.instruction("sub x6, x2, x4");                                      // compute bytes remaining from the two-byte leader
    emitter.instruction("cmp x6, #2");                                          // is the sequence truncated before its continuation byte?
    emitter.instruction("b.lo __rt_mb_strtoupper_utf8_invalid");                 // copy a truncated leader through unchanged
    emitter.instruction("add x8, x4, #1");                                      // address index of the required continuation byte
    emitter.instruction("ldrb w7, [x1, x8]");                                   // load the two-byte sequence continuation
    emitter.instruction("and w8, w7, #0xC0");                                   // isolate the continuation-byte prefix
    emitter.instruction("cmp w8, #0x80");                                       // does the second byte have the required 10xxxxxx shape?
    emitter.instruction("b.ne __rt_mb_strtoupper_utf8_invalid");                 // copy the malformed leader through unchanged
    emitter.instruction("and w0, w5, #0x1F");                                   // start the two-byte code point from the leader payload
    emitter.instruction("lsl w0, w0, #6");                                      // shift the leader payload into place
    emitter.instruction("and w7, w7, #0x3F");                                   // isolate the continuation payload
    emitter.instruction("orr w0, w0, w7");                                      // assemble the two-byte scalar
    emitter.instruction("mov x16, #2");                                         // two source bytes are consumed
    emitter.instruction("b __rt_mb_strtoupper_utf8_map");                       // apply Unicode uppercase

    emitter.label("__rt_mb_strtoupper_utf8_three");
    emitter.instruction("sub x6, x2, x4");                                      // compute bytes remaining from the three-byte leader
    emitter.instruction("cmp x6, #3");                                          // are all two continuation bytes available?
    emitter.instruction("b.lo __rt_mb_strtoupper_utf8_invalid");                 // copy a truncated leader through unchanged
    emitter.instruction("add x8, x4, #1");                                      // address index of the first continuation byte
    emitter.instruction("ldrb w7, [x1, x8]");                                   // load the first three-byte continuation
    emitter.instruction("and w8, w7, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w8, #0x80");                                       // is the first continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strtoupper_utf8_invalid");                 // copy the malformed leader through unchanged
    emitter.instruction("cmp w5, #0xE0");                                       // E0 requires a second byte at least A0 to avoid overlong UTF-8
    emitter.instruction("b.ne __rt_mb_strtoupper_utf8_three_not_e0");            // skip the E0 lower-bound check for other leaders
    emitter.instruction("cmp w7, #0xA0");                                       // is the E0 continuation inside the non-overlong range?
    emitter.instruction("b.lo __rt_mb_strtoupper_utf8_invalid");                 // copy an overlong three-byte sequence through
    emitter.label("__rt_mb_strtoupper_utf8_three_not_e0");
    emitter.instruction("cmp w5, #0xED");                                       // ED requires a second byte below A0 to exclude UTF-16 surrogates
    emitter.instruction("b.ne __rt_mb_strtoupper_utf8_three_second");            // skip the surrogate bound for other leaders
    emitter.instruction("cmp w7, #0xA0");                                       // does the ED continuation enter the surrogate range?
    emitter.instruction("b.hs __rt_mb_strtoupper_utf8_invalid");                 // copy UTF-8 encodings of surrogate code points through
    emitter.label("__rt_mb_strtoupper_utf8_three_second");
    emitter.instruction("add x8, x4, #2");                                      // address index of the second continuation byte
    emitter.instruction("ldrb w6, [x1, x8]");                                   // load the final three-byte continuation
    emitter.instruction("and w8, w6, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w8, #0x80");                                       // is the final continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strtoupper_utf8_invalid");                 // copy the malformed leader through unchanged
    emitter.instruction("and w0, w5, #0x0F");                                   // start the three-byte code point from the leader payload
    emitter.instruction("lsl w0, w0, #12");                                     // shift the leader payload into place
    emitter.instruction("and w7, w7, #0x3F");                                   // isolate the first continuation payload
    emitter.instruction("lsl w7, w7, #6");                                      // shift the first continuation payload into place
    emitter.instruction("orr w0, w0, w7");                                      // merge the first continuation
    emitter.instruction("and w6, w6, #0x3F");                                   // isolate the final continuation payload
    emitter.instruction("orr w0, w0, w6");                                      // assemble the three-byte scalar
    emitter.instruction("mov x16, #3");                                         // three source bytes are consumed
    emitter.instruction("b __rt_mb_strtoupper_utf8_map");                       // apply Unicode uppercase

    emitter.label("__rt_mb_strtoupper_utf8_four");
    emitter.instruction("sub x6, x2, x4");                                      // compute bytes remaining from the four-byte leader
    emitter.instruction("cmp x6, #4");                                          // are all three continuation bytes available?
    emitter.instruction("b.lo __rt_mb_strtoupper_utf8_invalid");                 // copy a truncated leader through unchanged
    emitter.instruction("add x8, x4, #1");                                      // address index of the first continuation byte
    emitter.instruction("ldrb w7, [x1, x8]");                                   // load the first four-byte continuation
    emitter.instruction("and w8, w7, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w8, #0x80");                                       // is the first continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strtoupper_utf8_invalid");                 // copy the malformed leader through unchanged
    emitter.instruction("cmp w5, #0xF0");                                       // F0 requires a second byte at least 90 to avoid overlong UTF-8
    emitter.instruction("b.ne __rt_mb_strtoupper_utf8_four_not_f0");             // skip the F0 lower-bound check for other leaders
    emitter.instruction("cmp w7, #0x90");                                       // is the F0 continuation inside the non-overlong range?
    emitter.instruction("b.lo __rt_mb_strtoupper_utf8_invalid");                 // copy an overlong four-byte sequence through
    emitter.label("__rt_mb_strtoupper_utf8_four_not_f0");
    emitter.instruction("cmp w5, #0xF4");                                       // F4 requires a second byte below 90 for Unicode's maximum scalar
    emitter.instruction("b.ne __rt_mb_strtoupper_utf8_four_rest");               // skip the upper bound for F0-F3
    emitter.instruction("cmp w7, #0x90");                                       // does the F4 continuation exceed U+10FFFF?
    emitter.instruction("b.hs __rt_mb_strtoupper_utf8_invalid");                 // copy out-of-range four-byte sequences through
    emitter.label("__rt_mb_strtoupper_utf8_four_rest");
    emitter.instruction("add x8, x4, #2");                                      // address index of the second continuation byte
    emitter.instruction("ldrb w6, [x1, x8]");                                   // load the second four-byte continuation
    emitter.instruction("and w8, w6, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w8, #0x80");                                       // is the second continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strtoupper_utf8_invalid");                 // copy the malformed leader through unchanged
    emitter.instruction("add x8, x4, #3");                                      // address index of the third continuation byte
    emitter.instruction("ldrb w3, [x1, x8]");                                   // load the final four-byte continuation
    emitter.instruction("and w8, w3, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w8, #0x80");                                       // is the final continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strtoupper_utf8_invalid");                 // copy the malformed leader through unchanged
    emitter.instruction("and w0, w5, #0x07");                                   // start the four-byte code point from the leader payload
    emitter.instruction("lsl w0, w0, #18");                                     // shift the leader payload into place
    emitter.instruction("and w7, w7, #0x3F");                                   // isolate the first continuation payload
    emitter.instruction("lsl w7, w7, #12");                                     // shift the first continuation payload into place
    emitter.instruction("orr w0, w0, w7");                                      // merge the first continuation
    emitter.instruction("and w6, w6, #0x3F");                                   // isolate the second continuation payload
    emitter.instruction("lsl w6, w6, #6");                                      // shift the second continuation payload into place
    emitter.instruction("orr w0, w0, w6");                                      // merge the second continuation
    emitter.instruction("and w3, w3, #0x3F");                                   // isolate the final continuation payload
    emitter.instruction("orr w0, w0, w3");                                      // assemble the four-byte scalar
    emitter.instruction("mov x16, #4");                                         // four source bytes are consumed
    emitter.instruction("b __rt_mb_strtoupper_utf8_map");                       // apply Unicode uppercase

    emitter.label("__rt_mb_strtoupper_utf8_map");
    emitter.instruction("str x16, [sp, #56]");                                  // preserve the consumed source-byte count across the lookup
    emitter.instruction("add x1, sp, #144");                                    // case-map destination is the 16-byte stack buffer
    emitter.instruction("bl __rt_mb_case_upper");                               // expand the scalar through Unicode full uppercase
    emitter.instruction("mov x17, x0");                                         // preserve the number of uppercase scalars
    emitter.instruction("add x14, sp, #144");                                   // reload the case-map output buffer
    emitter.instruction("mov x15, #0");                                         // uppercase-scalar index starts at zero
    emitter.label("__rt_mb_strtoupper_utf8_encode");
    emitter.instruction("cmp x15, x17");                                        // encoded every uppercase scalar?
    emitter.instruction("b.hs __rt_mb_strtoupper_utf8_mapped");                  // consume the source sequence after encoding
    emitter.instruction("ldr w0, [x14, x15, lsl #2]");                          // load the next uppercase scalar
    emitter.instruction("bl __rt_mb_strtoupper_put_utf8");                      // append its UTF-8 encoding to the destination
    emitter.instruction("add x15, x15, #1");                                    // advance the uppercase-scalar index
    emitter.instruction("b __rt_mb_strtoupper_utf8_encode");                    // encode the remaining uppercase scalars
    emitter.label("__rt_mb_strtoupper_utf8_mapped");
    emitter.instruction("ldr x16, [sp, #56]");                                  // restore the consumed source-byte count
    emitter.instruction("ldr x4, [sp, #16]");                                   // load the source byte index
    emitter.instruction("add x4, x4, x16");                                     // consume the mapped source sequence
    emitter.instruction("str x4, [sp, #16]");                                   // persist the advanced source index
    emitter.instruction("b __rt_mb_strtoupper_utf8_loop");                      // continue scanning the remaining bytes

    emitter.label("__rt_mb_strtoupper_utf8_invalid");
    emitter.instruction("ldr x9, [sp, #24]");                                   // load the destination pointer
    emitter.instruction("ldr x10, [sp, #40]");                                  // load the number of bytes already written
    emitter.instruction("strb w5, [x9, x10]");                                  // copy the malformed byte through unchanged
    emitter.instruction("add x10, x10, #1");                                    // account for the copied malformed byte
    emitter.instruction("str x10, [sp, #40]");                                  // persist the updated written length
    emitter.instruction("ldr x4, [sp, #16]");                                   // load the source byte index
    emitter.instruction("add x4, x4, #1");                                      // consume the malformed byte
    emitter.instruction("str x4, [sp, #16]");                                   // persist the advanced source index
    emitter.instruction("b __rt_mb_strtoupper_utf8_loop");                      // continue scanning the remaining bytes
}

/// Emits ASCII-only uppercase for PHP's 8bit/binary/7bit encodings.
fn emit_bytes_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtoupper_bytes");
    emitter.instruction("ldr x2, [sp, #8]");                                    // load the source length
    emitter.instruction("cbz x2, __rt_mb_strtoupper_empty");                    // empty input produces an empty result
    emitter.instruction("mov x0, x2");                                          // byte encodings never grow
    emitter.instruction("str x0, [sp, #32]");                                   // remember the reserved destination capacity
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage
    emitter.instruction("str x0, [sp, #24]");                                   // save the destination pointer
    emitter.instruction("str xzr, [sp, #40]");                                  // written length starts at zero
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the source length
    emitter.instruction("mov x4, #0");                                          // byte index starts at zero
    emitter.label("__rt_mb_strtoupper_bytes_loop");
    emitter.instruction("cmp x4, x2");                                          // copied every source byte?
    emitter.instruction("b.hs __rt_mb_strtoupper_bytes_done");                   // publish once the source is exhausted
    emitter.instruction("ldrb w5, [x1, x4]");                                   // load the next source byte
    emitter.instruction("cmp w5, #97");                                         // compare with 'a'
    emitter.instruction("b.lo __rt_mb_strtoupper_bytes_store");                  // bytes below 'a' stay unchanged
    emitter.instruction("cmp w5, #122");                                        // compare with 'z'
    emitter.instruction("b.hi __rt_mb_strtoupper_bytes_store");                  // bytes above 'z' stay unchanged
    emitter.instruction("sub w5, w5, #32");                                     // convert a-z to A-Z
    emitter.label("__rt_mb_strtoupper_bytes_store");
    emitter.instruction("strb w5, [x0, x4]");                                   // store the possibly uppercased byte
    emitter.instruction("add x4, x4, #1");                                      // advance the byte index
    emitter.instruction("b __rt_mb_strtoupper_bytes_loop");                     // continue the ASCII fold
    emitter.label("__rt_mb_strtoupper_bytes_done");
    emitter.instruction("str x2, [sp, #40]");                                   // the result length equals the source length
    emitter.instruction("b __rt_mb_strtoupper_finish");                         // publish the ASCII-uppercased copy
}

/// Emits iconv-backed uppercase for encodings other than UTF-8 and the byte aliases.
fn emit_iconv_aarch64(emitter: &mut Emitter, errno_function: &str) {
    emitter.label("__rt_mb_strtoupper_iconv");
    emitter.instruction("str xzr, [sp, #72]");                                  // no uppercase temporary exists until mapping allocates one
    abi::emit_symbol_address(emitter, "x0", "_mb_strlen_utf32le_name");
    emitter.instruction("add x1, sp, #80");                                     // iconv source encoding is the copied explicit name
    emitter.bl_c("iconv_open"); // create the encoding-to-UTF-32LE conversion descriptor
    emitter.instruction("cmn x0, #1");                                          // did iconv_open return the `(iconv_t)-1` failure sentinel?
    emitter.instruction("b.eq __rt_mb_strtoupper_unknown_encoding");            // unknown encoding names raise PHP's ValueError
    emitter.instruction("str x0, [sp, #192]");                                  // preserve the decode descriptor
    emitter.instruction("ldr x0, [sp, #8]");                                    // load the source length
    emitter.instruction("add x0, x0, #1");                                      // keep one extra UTF-32 slot for a possible BOM
    emitter.instruction("lsl x0, x0, #2");                                      // four bytes per decoded scalar
    emitter.instruction("str x0, [sp, #56]");                                   // remember the decode-buffer capacity
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate a temporary UTF-32 decode buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned string/block
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the temporary so heap_free can release it
    emitter.instruction("str x0, [sp, #48]");                                   // save the decode buffer
    emitter.instruction("str xzr, [sp, #64]");                                  // decoded UTF-32 byte count starts at zero
    emitter.instruction("ldr x9, [sp, #0]");                                    // iconv input starts at the PHP string
    emitter.instruction("str x9, [sp, #160]");                                  // initialize iconv's mutable input pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // iconv still has the whole source to consume
    emitter.instruction("str x9, [sp, #168]");                                  // initialize iconv's mutable input-byte count

    emitter.label("__rt_mb_strtoupper_iconv_dec_loop");
    emitter.instruction("ldr x9, [sp, #168]");                                  // load remaining input bytes
    emitter.instruction("cbz x9, __rt_mb_strtoupper_iconv_decoded");             // decode is finished when input is exhausted
    emitter.instruction("ldr x9, [sp, #48]");                                   // decode-buffer base
    emitter.instruction("ldr x10, [sp, #64]");                                  // bytes already decoded
    emitter.instruction("add x9, x9, x10");                                     // current UTF-32 write cursor
    emitter.instruction("str x9, [sp, #176]");                                  // initialize iconv's mutable output pointer
    emitter.instruction("ldr x9, [sp, #56]");                                   // decode-buffer capacity
    emitter.instruction("sub x9, x9, x10");                                     // remaining output bytes
    emitter.instruction("cbz x9, __rt_mb_strtoupper_iconv_dec_grow");            // grow when the temporary is already full
    emitter.instruction("str x9, [sp, #184]");                                  // initialize iconv's mutable output-byte count
    emitter.instruction("ldr x0, [sp, #192]");                                  // iconv argument 0 is the decode descriptor
    emitter.instruction("add x1, sp, #160");                                    // iconv argument 1 is `&input_ptr`
    emitter.instruction("add x2, sp, #168");                                    // iconv argument 2 is `&input_bytes_left`
    emitter.instruction("add x3, sp, #176");                                    // iconv argument 3 is `&output_ptr`
    emitter.instruction("add x4, sp, #184");                                    // iconv argument 4 is `&output_bytes_left`
    emitter.bl_c("iconv"); // decode as many complete characters as fit
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the decode-buffer capacity
    emitter.instruction("ldr x10, [sp, #184]");                                 // remaining unused output bytes
    emitter.instruction("sub x9, x9, x10");                                     // written = capacity - unused (out_left started at capacity - written)
    emitter.instruction("str x9, [sp, #64]");                                   // persist the accumulated UTF-32 byte count
    emitter.instruction("cmn x0, #1");                                          // did iconv report an error condition?
    emitter.instruction("b.ne __rt_mb_strtoupper_iconv_dec_loop");               // successful progress continues
    emitter.instruction("ldr x9, [sp, #184]");                                  // remaining output capacity
    emitter.instruction("cbz x9, __rt_mb_strtoupper_iconv_dec_grow");            // a full output buffer needs a larger decode temporary
    emitter.bl_c(errno_function); // fetch the platform thread-local errno written by iconv
    emitter.instruction("ldr w9, [x0]");                                        // load iconv's errno value
    emitter.instruction("cmp w9, #22");                                         // EINVAL means a truncated final sequence
    emitter.instruction("b.eq __rt_mb_strtoupper_iconv_decoded");                // stop at a truncated suffix without inventing scalars
    emitter.instruction("ldr x9, [sp, #168]");                                  // bytes remaining at a malformed sequence
    emitter.instruction("cbz x9, __rt_mb_strtoupper_iconv_decoded");             // defensive completion
    emitter.instruction("ldr x10, [sp, #160]");                                 // current input pointer
    emitter.instruction("add x10, x10, #1");                                    // skip one malformed input byte
    emitter.instruction("str x10, [sp, #160]");                                 // persist the advanced input pointer
    emitter.instruction("sub x9, x9, #1");                                      // remove the malformed byte
    emitter.instruction("str x9, [sp, #168]");                                  // persist the reduced input byte count
    emitter.instruction("ldr x0, [sp, #192]");                                  // reload the decode descriptor
    emitter.instruction("mov x1, #0");                                          // reset iconv shift state
    emitter.instruction("mov x2, #0");                                          // no input participates in the reset
    emitter.instruction("mov x3, #0");                                          // no output participates in the reset
    emitter.instruction("mov x4, #0");                                          // no output count participates in the reset
    emitter.bl_c("iconv"); // reset stateful decoders after skipping one malformed byte
    emitter.instruction("b __rt_mb_strtoupper_iconv_dec_loop");                  // continue decoding

    emitter.label("__rt_mb_strtoupper_iconv_dec_grow");
    emitter.instruction("ldr x0, [sp, #56]");                                   // current decode-buffer capacity
    emitter.instruction("lsl x0, x0, #1");                                      // double the UTF-32 temporary
    emitter.instruction("add x0, x0, #16");                                     // plus a small extra so a zero-capacity buffer can grow
    emitter.instruction("str x0, [sp, #200]");                                  // persist the new capacity across the allocator call
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the grown decode buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the grown decode temporary
    emitter.instruction("ldr x9, [sp, #48]");                                   // old decode buffer
    emitter.instruction("ldr x10, [sp, #64]");                                  // bytes already decoded
    emitter.instruction("mov x11, #0");                                         // copy index
    emitter.label("__rt_mb_strtoupper_iconv_dec_copy");
    emitter.instruction("cmp x11, x10");                                        // copied every decoded byte?
    emitter.instruction("b.hs __rt_mb_strtoupper_iconv_dec_copied");             // replace the old buffer once the prefix is duplicated
    emitter.instruction("ldrb w12, [x9, x11]");                                 // load one decoded byte
    emitter.instruction("strb w12, [x0, x11]");                                 // store it into the grown buffer
    emitter.instruction("add x11, x11, #1");                                    // advance the copy index
    emitter.instruction("b __rt_mb_strtoupper_iconv_dec_copy");                  // continue the prefix copy
    emitter.label("__rt_mb_strtoupper_iconv_dec_copied");
    emitter.instruction("str x0, [sp, #48]");                                   // persist the grown decode buffer
    emitter.instruction("ldr x0, [sp, #200]");                                  // reload the new capacity
    emitter.instruction("str x0, [sp, #56]");                                   // persist the grown capacity
    emitter.instruction("mov x0, x9");                                          // release the superseded decode buffer
    emitter.instruction("bl __rt_heap_free");                                   // return the old temporary to the heap
    emitter.instruction("b __rt_mb_strtoupper_iconv_dec_loop");                  // retry the conversion with the larger buffer

    emitter.label("__rt_mb_strtoupper_iconv_decoded");
    emitter.instruction("ldr x0, [sp, #192]");                                  // close the decode descriptor
    emitter.bl_c("iconv_close"); // release the encoding-to-UTF-32LE descriptor
    emitter.instruction("ldr x0, [sp, #64]");                                   // allocate 3x the decoded UTF-32 for 1:N growth
    emitter.instruction("add x0, x0, x0, lsl #1");                              // dest_cap = decoded_len * 3
    emitter.instruction("cmp x0, #16");                                         // keep a minimum uppercase buffer
    emitter.instruction("b.hs __rt_mb_strtoupper_iconv_upper_cap");              // use the 3x size when it is large enough
    emitter.instruction("mov x0, #16");                                         // minimum 16-byte uppercase buffer
    emitter.label("__rt_mb_strtoupper_iconv_upper_cap");
    emitter.instruction("str x0, [sp, #232]");                                  // remember the uppercase-buffer capacity
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the uppercase UTF-32 buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the uppercase temporary
    emitter.instruction("str x0, [sp, #72]");                                   // save the uppercase buffer
    emitter.instruction("str xzr, [sp, #224]");                                 // uppercase written byte count starts at zero
    emitter.instruction("str xzr, [sp, #208]");                                 // decode byte index starts at zero
    emitter.instruction("ldr x9, [sp, #64]");                                   // decoded UTF-32 byte count
    emitter.instruction("str x9, [sp, #216]");                                  // persist the decode length for the mapping loop
    emitter.instruction("str xzr, [sp, #200]");                                 // saw_payload starts false so a leading BOM can be skipped

    emitter.label("__rt_mb_strtoupper_iconv_map_loop");
    emitter.instruction("ldr x11, [sp, #208]");                                 // load the decode byte index
    emitter.instruction("ldr x10, [sp, #216]");                                 // load the decoded length
    emitter.instruction("cmp x11, x10");                                        // mapped every decoded scalar?
    emitter.instruction("b.hs __rt_mb_strtoupper_iconv_mapped");                 // encode once uppercase is finished
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the decode buffer
    emitter.instruction("ldr w0, [x9, x11]");                                   // load the next UTF-32LE scalar
    emitter.instruction("add x11, x11, #4");                                    // consume four decode bytes
    emitter.instruction("str x11, [sp, #208]");                                 // persist the advanced decode index
    emitter.instruction("ldr x12, [sp, #200]");                                 // load the saw_payload flag
    emitter.instruction("cbnz x12, __rt_mb_strtoupper_iconv_map_apply");         // after the first payload scalar, keep every later BOM
    emitter.instruction("mov w12, #0xFEFF");                                    // UTF-32LE BOM scalar
    emitter.instruction("cmp w0, w12");                                         // is this a leading UTF-32 BOM?
    emitter.instruction("b.eq __rt_mb_strtoupper_iconv_map_loop");               // skip a leading BOM
    emitter.label("__rt_mb_strtoupper_iconv_map_apply");
    emitter.instruction("mov x12, #1");                                         // later scalars are payload
    emitter.instruction("str x12, [sp, #200]");                                 // persist saw_payload
    emitter.instruction("add x1, sp, #144");                                    // case-map destination is the 16-byte stack buffer
    emitter.instruction("bl __rt_mb_case_upper");                               // expand the scalar through Unicode full uppercase
    emitter.instruction("lsl x1, x0, #2");                                      // each uppercase scalar occupies four UTF-32 bytes
    emitter.instruction("ldr x10, [sp, #224]");                                 // bytes already written to the uppercase buffer
    emitter.instruction("ldr x11, [sp, #232]");                                 // uppercase-buffer capacity
    emitter.instruction("add x12, x10, x1");                                    // the bytes this mapping would occupy
    emitter.instruction("cmp x12, x11");                                        // does the mapping still fit?
    emitter.instruction("b.ls __rt_mb_strtoupper_iconv_map_store");              // copy when the current buffer is large enough
    emitter.instruction("str x0, [sp, #192]");                                  // preserve the uppercase scalar count across grow
    emitter.instruction("str x1, [sp, #176]");                                  // preserve the mapping byte count across grow
    emitter.instruction("lsl x0, x11, #1");                                     // double the uppercase capacity
    emitter.instruction("add x0, x0, #16");                                     // plus a small extra so tiny buffers can grow
    emitter.instruction("str x0, [sp, #232]");                                  // persist the new uppercase capacity
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the grown uppercase buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the grown uppercase temporary
    emitter.instruction("ldr x9, [sp, #72]");                                   // old uppercase buffer
    emitter.instruction("ldr x10, [sp, #224]");                                 // bytes already written
    emitter.instruction("mov x11, #0");                                         // copy index
    emitter.label("__rt_mb_strtoupper_iconv_upper_copy");
    emitter.instruction("cmp x11, x10");                                        // copied every uppercase byte?
    emitter.instruction("b.hs __rt_mb_strtoupper_iconv_upper_copied");           // replace the old buffer once the prefix is duplicated
    emitter.instruction("ldrb w12, [x9, x11]");                                 // load one uppercase byte
    emitter.instruction("strb w12, [x0, x11]");                                 // store it into the grown buffer
    emitter.instruction("add x11, x11, #1");                                    // advance the copy index
    emitter.instruction("b __rt_mb_strtoupper_iconv_upper_copy");                // continue the prefix copy
    emitter.label("__rt_mb_strtoupper_iconv_upper_copied");
    emitter.instruction("str x0, [sp, #72]");                                   // persist the grown uppercase buffer
    emitter.instruction("mov x0, x9");                                          // release the superseded uppercase buffer
    emitter.instruction("bl __rt_heap_free");                                   // return the old temporary to the heap
    emitter.instruction("ldr x0, [sp, #192]");                                  // restore the uppercase scalar count
    emitter.instruction("ldr x1, [sp, #176]");                                  // restore the mapping byte count
    emitter.label("__rt_mb_strtoupper_iconv_map_store");
    emitter.instruction("ldr x9, [sp, #72]");                                   // uppercase buffer
    emitter.instruction("ldr x10, [sp, #224]");                                 // bytes already written
    emitter.instruction("add x9, x9, x10");                                     // write cursor
    emitter.instruction("add x11, sp, #144");                                   // case-map output
    emitter.instruction("mov x12, #0");                                         // copy index
    emitter.label("__rt_mb_strtoupper_iconv_map_copy");
    emitter.instruction("cmp x12, x1");                                         // copied every mapped UTF-32 byte?
    emitter.instruction("b.hs __rt_mb_strtoupper_iconv_map_copied");             // persist the written length once the mapping is stored
    emitter.instruction("ldrb w13, [x11, x12]");                                // load one mapped byte
    emitter.instruction("strb w13, [x9, x12]");                                 // store it into the uppercase buffer
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_mb_strtoupper_iconv_map_copy");                  // continue the mapping copy
    emitter.label("__rt_mb_strtoupper_iconv_map_copied");
    emitter.instruction("add x10, x10, x1");                                    // account for the mapped UTF-32 bytes
    emitter.instruction("str x10, [sp, #224]");                                 // persist the updated uppercase length
    emitter.instruction("b __rt_mb_strtoupper_iconv_map_loop");                  // continue mapping the remaining scalars

    emitter.label("__rt_mb_strtoupper_iconv_mapped");
    emitter.instruction("ldr x9, [sp, #224]");                                  // uppercase UTF-32 byte count
    emitter.instruction("cbnz x9, __rt_mb_strtoupper_iconv_encode");             // encode when mapping produced at least one scalar
    emitter.instruction("bl __rt_mb_strtoupper_iconv_free_temps");               // release both UTF-32 temporaries
    emitter.instruction("b __rt_mb_strtoupper_empty");                          // an empty mapping produces an empty string

    emitter.label("__rt_mb_strtoupper_iconv_encode");
    emitter.instruction("add x0, sp, #80");                                     // encode destination encoding is the copied explicit name
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_utf32le_name");
    emitter.bl_c("iconv_open"); // create the UTF-32LE-to-encoding conversion descriptor
    emitter.instruction("cmn x0, #1");                                          // did the encoder reject the same encoding name?
    emitter.instruction("b.ne __rt_mb_strtoupper_iconv_encode_opened");          // continue when the encoder opened
    emitter.instruction("bl __rt_mb_strtoupper_iconv_free_temps");               // release both UTF-32 temporaries before unwinding
    emitter.instruction("b __rt_mb_strtoupper_unknown_encoding");                // treat a failed encoder open as an unknown encoding
    emitter.label("__rt_mb_strtoupper_iconv_encode_opened");
    emitter.instruction("str x0, [sp, #192]");                                  // preserve the encode descriptor
    emitter.instruction("ldr x0, [sp, #224]");                                  // reserve at least the uppercase UTF-32 byte count
    emitter.instruction("lsl x0, x0, #1");                                      // encodings can expand past UTF-32
    emitter.instruction("add x0, x0, #16");                                     // plus a small extra so tiny results can encode
    emitter.instruction("str x0, [sp, #32]");                                   // remember the reserved destination capacity
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage
    emitter.instruction("str x0, [sp, #24]");                                   // save the destination pointer
    emitter.instruction("str xzr, [sp, #40]");                                  // encoded byte count starts at zero
    emitter.instruction("ldr x9, [sp, #72]");                                   // encode input starts at the uppercase UTF-32
    emitter.instruction("str x9, [sp, #160]");                                  // initialize iconv's mutable input pointer
    emitter.instruction("ldr x9, [sp, #224]");                                  // encode still has the whole uppercase buffer
    emitter.instruction("str x9, [sp, #168]");                                  // initialize iconv's mutable input-byte count

    emitter.label("__rt_mb_strtoupper_iconv_enc_loop");
    emitter.instruction("ldr x9, [sp, #168]");                                  // load remaining uppercase UTF-32 bytes
    emitter.instruction("cbz x9, __rt_mb_strtoupper_iconv_encoded");             // encode is finished when input is exhausted
    emitter.instruction("ldr x9, [sp, #24]");                                   // destination base
    emitter.instruction("ldr x10, [sp, #40]");                                  // bytes already encoded
    emitter.instruction("add x9, x9, x10");                                     // current encode write cursor
    emitter.instruction("str x9, [sp, #176]");                                  // initialize iconv's mutable output pointer
    emitter.instruction("ldr x9, [sp, #32]");                                   // destination capacity
    emitter.instruction("sub x9, x9, x10");                                     // remaining destination bytes
    emitter.instruction("cbz x9, __rt_mb_strtoupper_iconv_enc_grow");            // grow when the destination is already full
    emitter.instruction("str x9, [sp, #184]");                                  // initialize iconv's mutable output-byte count
    emitter.instruction("ldr x0, [sp, #192]");                                  // iconv argument 0 is the encode descriptor
    emitter.instruction("add x1, sp, #160");                                    // iconv argument 1 is `&input_ptr`
    emitter.instruction("add x2, sp, #168");                                    // iconv argument 2 is `&input_bytes_left`
    emitter.instruction("add x3, sp, #176");                                    // iconv argument 3 is `&output_ptr`
    emitter.instruction("add x4, sp, #184");                                    // iconv argument 4 is `&output_bytes_left`
    emitter.bl_c("iconv"); // encode as many complete characters as fit
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the destination capacity
    emitter.instruction("ldr x10, [sp, #184]");                                 // remaining unused destination bytes
    emitter.instruction("sub x9, x9, x10");                                     // written = capacity - unused
    emitter.instruction("str x9, [sp, #40]");                                   // persist the accumulated encoded byte count
    emitter.instruction("cmn x0, #1");                                          // did iconv report an error condition?
    emitter.instruction("b.ne __rt_mb_strtoupper_iconv_enc_loop");               // successful progress continues
    emitter.instruction("ldr x9, [sp, #184]");                                  // remaining destination capacity
    emitter.instruction("cbz x9, __rt_mb_strtoupper_iconv_enc_grow");            // a full destination needs a larger reservation
    emitter.bl_c(errno_function); // fetch the platform thread-local errno written by iconv
    emitter.instruction("ldr w9, [x0]");                                        // load iconv's errno value
    emitter.instruction("cmp w9, #22");                                         // EINVAL means a truncated final sequence
    emitter.instruction("b.eq __rt_mb_strtoupper_iconv_encoded");                // stop at a truncated suffix
    emitter.instruction("ldr x9, [sp, #168]");                                  // bytes remaining at a malformed sequence
    emitter.instruction("cbz x9, __rt_mb_strtoupper_iconv_encoded");             // defensive completion
    emitter.instruction("ldr x10, [sp, #160]");                                 // current input pointer
    emitter.instruction("add x10, x10, #1");                                    // skip one malformed input byte
    emitter.instruction("str x10, [sp, #160]");                                 // persist the advanced input pointer
    emitter.instruction("sub x9, x9, #1");                                      // remove the malformed byte
    emitter.instruction("str x9, [sp, #168]");                                  // persist the reduced input byte count
    emitter.instruction("ldr x0, [sp, #192]");                                  // reload the encode descriptor
    emitter.instruction("mov x1, #0");                                          // reset iconv shift state
    emitter.instruction("mov x2, #0");                                          // no input participates in the reset
    emitter.instruction("mov x3, #0");                                          // no output participates in the reset
    emitter.instruction("mov x4, #0");                                          // no output count participates in the reset
    emitter.bl_c("iconv"); // reset stateful encoders after skipping one malformed byte
    emitter.instruction("b __rt_mb_strtoupper_iconv_enc_loop");                  // continue encoding

    emitter.label("__rt_mb_strtoupper_iconv_enc_grow");
    emitter.instruction("ldr x0, [sp, #24]");                                   // current destination buffer
    emitter.instruction("ldr x1, [sp, #40]");                                   // preserve the bytes already encoded
    emitter.instruction("ldr x2, [sp, #32]");                                   // current destination capacity
    emitter.instruction("lsl x2, x2, #1");                                      // double the destination capacity
    emitter.instruction("add x2, x2, #16");                                     // plus a small extra so tiny results can grow
    emitter.instruction("str x2, [sp, #32]");                                   // persist the new capacity before the grow call
    emitter.instruction("bl __rt_concat_grow");                                 // replace the destination with a larger owned buffer
    emitter.instruction("str x0, [sp, #24]");                                   // persist the grown destination pointer
    emitter.instruction("b __rt_mb_strtoupper_iconv_enc_loop");                  // retry the conversion with the larger destination

    emitter.label("__rt_mb_strtoupper_iconv_encoded");
    emitter.instruction("ldr x0, [sp, #192]");                                  // close the encode descriptor
    emitter.bl_c("iconv_close"); // release the UTF-32LE-to-encoding descriptor
    emitter.instruction("bl __rt_mb_strtoupper_iconv_free_temps");               // release both UTF-32 temporaries
    emitter.instruction("b __rt_mb_strtoupper_finish");                         // publish the encoded uppercase string

    emitter.label("__rt_mb_strtoupper_iconv_free_temps");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the helper return address across the two frees
    emitter.instruction("ldr x0, [sp, #64]");                                   // decode buffer is at the original sp+48, now +16
    emitter.instruction("bl __rt_heap_free");                                   // release the decode temporary
    emitter.instruction("ldr x0, [sp, #88]");                                   // uppercase buffer is at the original sp+72, now +16
    emitter.instruction("cbz x0, __rt_mb_strtoupper_iconv_free_done");           // skip when mapping never allocated
    emitter.instruction("bl __rt_heap_free");                                   // release the uppercase temporary
    emitter.label("__rt_mb_strtoupper_iconv_free_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the helper return address
    emitter.instruction("ret");                                                 // return to the iconv path
}

/// Emits destination-capacity growth and UTF-8 append helpers used by the walker.
fn emit_helpers_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtoupper_ensure");
    emitter.instruction("ldr x10, [sp, #40]");                                  // load the number of destination bytes already written
    emitter.instruction("ldr x11, [sp, #32]");                                  // load the reserved destination capacity
    emitter.instruction("add x12, x10, #12");                                   // the next scalar can emit up to twelve UTF-8 bytes
    emitter.instruction("cmp x12, x11");                                        // does the reservation still have twelve free bytes?
    emitter.instruction("b.ls __rt_mb_strtoupper_ensure_done");                  // keep the current reservation when it still fits
    emitter.instruction("ldr x0, [sp, #24]");                                   // current destination buffer
    emitter.instruction("mov x1, x10");                                         // preserve the bytes already written
    emitter.instruction("lsl x2, x11, #1");                                     // double the destination capacity
    emitter.instruction("add x2, x2, #16");                                     // plus a small extra so tiny strings can grow
    emitter.instruction("str x2, [sp, #32]");                                   // persist the new capacity before the grow call
    emitter.instruction("bl __rt_concat_grow");                                 // replace the destination with a larger owned buffer
    emitter.instruction("str x0, [sp, #24]");                                   // persist the grown destination pointer
    emitter.label("__rt_mb_strtoupper_ensure_done");
    emitter.instruction("ret");                                                 // return to the UTF-8 walker

    emitter.label("__rt_mb_strtoupper_put_utf8");
    emitter.instruction("ldr x9, [sp, #24]");                                   // load the destination pointer
    emitter.instruction("ldr x10, [sp, #40]");                                  // load the number of bytes already written
    emitter.instruction("cmp w0, #0x80");                                       // one-byte UTF-8?
    emitter.instruction("b.hs __rt_mb_strtoupper_put_utf8_2");                   // encode a longer sequence
    emitter.instruction("strb w0, [x9, x10]");                                  // store the ASCII byte
    emitter.instruction("add x10, x10, #1");                                    // one byte was written
    emitter.instruction("str x10, [sp, #40]");                                  // persist the updated written length
    emitter.instruction("ret");                                                 // return after the one-byte encode
    emitter.label("__rt_mb_strtoupper_put_utf8_2");
    emitter.instruction("cmp w0, #0x800");                                      // two-byte UTF-8?
    emitter.instruction("b.hs __rt_mb_strtoupper_put_utf8_3");                   // encode a three- or four-byte sequence
    emitter.instruction("lsr w11, w0, #6");                                     // high five bits
    emitter.instruction("orr w11, w11, #0xC0");                                 // two-byte leader
    emitter.instruction("strb w11, [x9, x10]");                                 // store the two-byte leader
    emitter.instruction("and w11, w0, #0x3F");                                  // low six bits
    emitter.instruction("orr w11, w11, #0x80");                                 // continuation shape
    emitter.instruction("add x10, x10, #1");                                    // advance to the continuation slot
    emitter.instruction("strb w11, [x9, x10]");                                 // store the continuation
    emitter.instruction("add x10, x10, #1");                                    // two bytes were written
    emitter.instruction("str x10, [sp, #40]");                                  // persist the updated written length
    emitter.instruction("ret");                                                 // return after the two-byte encode
    emitter.label("__rt_mb_strtoupper_put_utf8_3");
    emitter.instruction("cmp w0, #0x10000");                                    // three-byte UTF-8?
    emitter.instruction("b.hs __rt_mb_strtoupper_put_utf8_4");                   // encode a four-byte sequence
    emitter.instruction("lsr w11, w0, #12");                                    // high four bits
    emitter.instruction("orr w11, w11, #0xE0");                                 // three-byte leader
    emitter.instruction("strb w11, [x9, x10]");                                 // store the three-byte leader
    emitter.instruction("lsr w11, w0, #6");                                     // middle six bits
    emitter.instruction("and w11, w11, #0x3F");                                 // isolate the continuation payload
    emitter.instruction("orr w11, w11, #0x80");                                 // continuation shape
    emitter.instruction("add x12, x10, #1");                                    // address the second byte
    emitter.instruction("strb w11, [x9, x12]");                                 // store the first continuation
    emitter.instruction("and w11, w0, #0x3F");                                  // low six bits
    emitter.instruction("orr w11, w11, #0x80");                                 // continuation shape
    emitter.instruction("add x12, x10, #2");                                    // address the third byte
    emitter.instruction("strb w11, [x9, x12]");                                 // store the final continuation
    emitter.instruction("add x10, x10, #3");                                    // three bytes were written
    emitter.instruction("str x10, [sp, #40]");                                  // persist the updated written length
    emitter.instruction("ret");                                                 // return after the three-byte encode
    emitter.label("__rt_mb_strtoupper_put_utf8_4");
    emitter.instruction("lsr w11, w0, #18");                                    // high three bits
    emitter.instruction("orr w11, w11, #0xF0");                                 // four-byte leader
    emitter.instruction("strb w11, [x9, x10]");                                 // store the four-byte leader
    emitter.instruction("lsr w11, w0, #12");                                    // next six bits
    emitter.instruction("and w11, w11, #0x3F");                                 // isolate the continuation payload
    emitter.instruction("orr w11, w11, #0x80");                                 // continuation shape
    emitter.instruction("add x12, x10, #1");                                    // address the second byte
    emitter.instruction("strb w11, [x9, x12]");                                 // store the first continuation
    emitter.instruction("lsr w11, w0, #6");                                     // next six bits
    emitter.instruction("and w11, w11, #0x3F");                                 // isolate the continuation payload
    emitter.instruction("orr w11, w11, #0x80");                                 // continuation shape
    emitter.instruction("add x12, x10, #2");                                    // address the third byte
    emitter.instruction("strb w11, [x9, x12]");                                 // store the second continuation
    emitter.instruction("and w11, w0, #0x3F");                                  // low six bits
    emitter.instruction("orr w11, w11, #0x80");                                 // continuation shape
    emitter.instruction("add x12, x10, #3");                                    // address the fourth byte
    emitter.instruction("strb w11, [x9, x12]");                                 // store the final continuation
    emitter.instruction("add x10, x10, #4");                                    // four bytes were written
    emitter.instruction("str x10, [sp, #40]");                                  // persist the updated written length
    emitter.instruction("ret");                                                 // return after the four-byte encode
}
